# Tekton with btdt-server

This guide explains how to use `btdt` in a [Tekton](https://tekton.dev/) pipeline with a `btdt-server` deployed into
your Kubernetes
cluster.
It will use the Docker images, so that no changes to the images of your tasks are necessary.
Of course, you could also install `btdt` within the respective task images which might simplify the integration a bit.

For other options, to integrate `btdt` into Tekton, check out the [Tekton overview](./tekton.md).

## Deploy btdt-server

First, we create all the Kubernetes resources for a `btdt-server` deployment.

### PVC for the cache

The `btdt-server` needs a persistent volume claim (PVC) to store the cache.

```yaml
# btdt-server-pvc.yaml
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: btdt-cache-pvc
spec:
  accessModes:
    - ReadWriteOnce
  resources:
    requests:
      # The required storage size depends on your use case.
      storage: 10Gi
```

### Authorization secrets

For authorization of `btdt` clients against the `btdt-server`, we need to create a private key and an authorization
token. See the [Authorization](../btdt-server/authorization.md) documentation for details on how to do this.

```sh
cargo install biscuit-cli
# Generate a new private key and save it to a file
biscuit keypair --key-output-format pem --only-private-key | head -c -1 > auth_private_key.pem
# Generate a new authorization token with all permissions and validity of 90 days
biscuit generate \
  --private-key-file auth_private_key.pem \
  --private-key-format pem \
  --add-ttl 90d - <<EOF | base64 > auth_token.txt
EOF
```

Use these to create the following Kubernetes secrets:

```yaml
# btdt-auth-key.yaml
apiVersion: v1
kind: Secret
metadata:
  name: btdt-auth-key
type: Opaque
stringData:
  auth_private_key.pem: |
    -----BEGIN PRIVATE KEY-----
    <your private key here>
    -----END PRIVATE KEY-----
```

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: btdt-client-token
type: Opaque
data:
  token: |
    <your authorization token here>
```

### Server configuration

The `btdt-server` needs a configuration file to specify the cache location and cleanup parameters.

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: btdt-server-config
data:
  config.toml: |
    [cleanup]
    interval = '1h'
    cache_expiration = '7days'
    max_cache_size = '10GiB'

    [caches]
    default = { type = 'Filesystem', path = '/var/lib/btdt/cache-default' }
```

### Deployment and service for the btdt-server

Now, we can create a deployment for the `btdt-server` itself. Because the `btdt-server` container image
is distroless and uses a non-root user, we need to use an init container to set the correct permissions for the private
key file.

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: btdt-server
spec:
  replicas: 1
  selector:
    matchLabels:
      app: btdt-server
  template:
    metadata:
      labels:
        app: btdt-server
    spec:
      initContainers:
        - name: setup-permissions
          image: busybox:1.36
          command: [ 'sh', '-c', 'cp /auth-key-secret/auth_private_key.pem /auth-key-work/auth_private_key.pem && chown 65532:65532 /auth-key-work/auth_private_key.pem && chmod 0600 /auth-key-work/auth_private_key.pem' ]
          volumeMounts:
            - name: auth-key
              mountPath: /auth-key-secret
            - name: auth-key-work
              mountPath: /auth-key-work
      volumes:
        - name: config
          configMap:
            name: btdt-server-config
        - name: auth-key
          secret:
            secretName: btdt-auth-key
        - name: auth-key-work
          emptyDir: { }
        - name: cache-storage
          persistentVolumeClaim:
            claimName: btdt-cache-pvc
      containers:
        - name: btdt-server
          image: jgosmann/btdt-server:0.4.1
          ports:
            - containerPort: 8707
              name: http
          env:
            - name: BTDT_AUTH_PRIVATE_KEY
              value: "/auth_private_key.pem"
            - name: BTDT_SERVER_CONFIG_FILE
              value: "/config.toml"
          resources:
            requests:
              cpu: 100m
              memory: 128Mi
            limits:
              cpu: 500m
              memory: 512Mi
          volumeMounts:
            - name: config
              mountPath: /config.toml
              subPath: config.toml
            - name: auth-key-work
              mountPath: /auth_private_key.pem
              subPath: auth_private_key.pem
            - name: cache-storage
              mountPath: /var/lib/btdt/cache-default
          securityContext:
            allowPrivilegeEscalation: false
            capabilities:
              drop:
                - ALL
            runAsNonRoot: true
            runAsUser: 65532
            seccompProfile:
              type: RuntimeDefault
```

Finally, we need a service definition:

```yaml
apiVersion: v1
kind: Service
metadata:
  name: btdt-server
spec:
  selector:
    app: btdt-server
  ports:
    - name: http
      protocol: TCP
      port: 8707
      targetPort: 8707
  type: ClusterIP
```

## Use the cache in a Tekton task

You must provdie a volume with the authorization token in your Tekton task, so that the `btdt` CLI can connect to the
server:

```yaml
# task_run-tests.yaml
apiVersion: tekton.dev/v1beta1
kind: Task
metadata:
  name: run-tests
spec:
  steps:
  # ...
  volumes:
    - name: btdt-token
      secret:
        secretName: btdt-client-token
        defaultMode: 0600
```

### Restore the cache

Now you can add a step to restore the cache at the beginning of the task.
Here, we try to restore a `node_modules` directory:

```yaml
# task_run-tests.yaml
spec:
  # ...
  steps:
    - name: restore-cache
      image: jgosmann/btdt:0.4.1-alpine
      workingDir: $(workspaces.git-sources.path)
      onError: continue
      script: |
        #!/bin/sh
        CACHE_KEY=node-modules-$(btdt hash package-lock.json)
        echo "Cache key: $CACHE_KEY"
        btdt restore \
          --cache http://btdt-server.default.svc.cluster.local:8707/api/caches/default \
          --auth-token-file /tmp/btdt-token/token \
          --keys $CACHE_KEY \
          node_modules
      volumeMounts:
        - name: btdt-token
          mountPath: /tmp/btdt-token
          readOnly: true
      securityContext:
        runAsUser: 65532
```

### Install dependencies only on cache miss

Depending on what you are caching, you might want to run some commands only a cache miss to generate the files that
would be cached.
For example, to install NPM dependencies only if the cache could not be restored:

```yaml
# task_run-tests.yaml
spec:
  # ...
  steps:
    # try restore
    - name: run-tests
      image: node
      workingDir: $(workspaces.git-sources.path)
      script: |
        #!/bin/sh
        if [ $(cat $(steps.step-restore-cache.exitCode.path)) -eq 0 ]; then
          echo "Cache restore succeeded, skipping npm ci"
        else
          npm ci
        fi
        # run tests, build, etc.
```

Note, if you are using [fallback keys](../getting_started.md#using-multiple-cache-keys), you would always want to run
`npm ci` to ensure that the dependencies are installed correctly.

### Store the cache

For the cache to provide a benefit, we need to fill it if a cache miss occurred. This requires an additional step
after the files to cache have been generated (e.g. by running `npm ci`):

```yaml
# task_run-tests.yaml
spec:
  # ...
  steps:
    # try restore
    # install dependencies/generate files to cache
    - name: store-cache
      image: jgosmann/btdt:0.4.1-alpine
      workingDir: $(workspaces.git-sources.path)
      script: |
        #!/bin/sh
        if [ $(cat $(steps.step-restore-cache.exitCode.path)) -eq 0 ]; then
            echo "Cache restore succeeded, skipping cache store"
            exit 0
        fi
        CACHE_KEY=node-modules-$(btdt hash package-lock.json)
        echo "Cache key: $CACHE_KEY"
        btdt store \
          --cache http://btdt-server.default.svc.cluster.local:8707/api/caches/default \
          --auth-token-file /tmp/btdt-token/token \
          --keys $CACHE_KEY \
          node_modules
      volumeMounts:
        - name: btdt-token
          mountPath: /tmp/btdt-token
          readOnly: true
      securityContext:
        runAsUser: 65532
```

### Example of complete task

When putting all of this together, your task definition will look something like this:

```yaml
# task_run-tests.yaml
apiVersion: tekton.dev/v1beta1
kind: Task
metadata:
  name: run-tests
spec:
  steps:
    - name: restore-cache
      image: jgosmann/btdt:0.4.1-alpine
      workingDir: $(workspaces.git-sources.path)
      onError: continue
      script: |
        #!/bin/sh
        CACHE_KEY=node-modules-$(btdt hash package-lock.json)
        echo "Cache key: $CACHE_KEY"
        btdt restore \
          --cache http://btdt-server.default.svc.cluster.local:8707/api/caches/default \
          --auth-token-file /tmp/btdt-token/token \
          --keys $CACHE_KEY \
          node_modules
      volumeMounts:
        - name: btdt-token
          mountPath: /tmp/btdt-token
          readOnly: true
      securityContext:
        runAsUser: 65532
    - name: run-tests
      image: node
      workingDir: $(workspaces.git-sources.path)
      script: |
        #!/bin/sh
        if [ $(cat $(steps.step-restore-cache.exitCode.path)) -eq 0 ]; then
          echo "Cache restore succeeded, skipping npm ci"
        else
          npm ci
        fi
        # run tests, build, etc.
    - name: store-cache
      image: jgosmann/btdt:0.4.1-alpine
      workingDir: $(workspaces.git-sources.path)
      script: |
        #!/bin/sh
        if [ $(cat $(steps.step-restore-cache.exitCode.path)) -eq 0 ]; then
            echo "Cache restore succeeded, skipping cache store"
            exit 0
        fi
        CACHE_KEY=node-modules-$(btdt hash package-lock.json)
        echo "Cache key: $CACHE_KEY"
        btdt store \
          --cache http://btdt-server.default.svc.cluster.local:8707/api/caches/default \
          --auth-token-file /tmp/btdt-token/token \
          --keys $CACHE_KEY \
          node_modules
      volumeMounts:
        - name: btdt-token
          mountPath: /tmp/btdt-token
          readOnly: true
      securityContext:
        runAsUser: 65532
  volumes:
    - name: btdt-token
      secret:
        secretName: btdt-client-token
        defaultMode: 0600
  workspaces:
    - name: git-sources
      description: Provides the workspace with the cloned repository.
```
