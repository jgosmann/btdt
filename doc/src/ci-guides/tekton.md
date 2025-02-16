# Tekton

This guide explains how to use `btdt` in a [Tekton](https://tekton.dev/) pipeline.
It will use the Docker images, so that no changes to the images of your tasks are necessary.
Of course, you could also install `btdt` within the respective task images which might simplify the integration a bit.

## Provide a Persistent Volume Claim as workspace to the pipeline run

To use `btdt` in a Tekton pipeline, you need to provide a Persistent Volume Claim (PVC) for the cache.
This PVC should be provided as actual `persistentVolumeClaim` in the `PipelineRun`, not `volumeClaimTemplate`.
Otherwise, you will have a fresh volume on each pipeline run, making the cache useless.
An example `PipelineRun` could look like this:

```yaml
# PipelineRun template, e.g. as part of your trigger
apiVersion: tekton.dev/v1beta1
kind: PipelineRun
metadata:
  name: my-pipeline-run-$(uid)
spec:
  params:
  # ...
  pipelineRef:
    name: my-tekton-pipeline
  workspaces:
    - name: cache
      persistentVolumeClaim:
        claimName: my-tekton-cache
```

With the default Tekton settings (at time of writing), only a single PVC can be mounted into a task.
Thus, if you are already using a PVC for you task (likely to check out your source code repository),
you will have to also store the cache on this PVC.

Alternatively, you can disable the [affinity assistant](https://tekton.dev/docs/pipelines/affinityassistants/) to be
able to mount multiple PVCs into a task. Run `kubectl edit configmap feature-flags` to edit the configuration.
In the following, we assume this second setup. If you are using a single PVC, you will have to adjust the paths
accordingly.

## Provide the cache workspace to the task

To be able to use the cache in a task, the cache workspace needs to be provided:

```yaml
# pipeline.yaml
apiVersion: tekton.dev/v1beta1
kind: Pipeline
metadata:
  name: my-tekton-pipeline
spec:
  workspaces:
    - name: cache
  tasks:
    - name: run-tests
      taskRef:
        name: run-tests
        kind: Task
      workspaces:
        - name: git-sources
          workspace: git-sources
        - name: cache
          workspace: cache
```

## Use the cache in a task

You must declare the cache workspace in the task, so that it can be used by the individual steps:

```yaml
# task_run-tests.yaml
apiVersion: tekton.dev/v1beta1
kind: Task
metadata:
  name: run-tests
spec:
  steps:
  # ...
  workspaces:
    - name: git-sources
      description: Provides the workspace with the cloned repository.
    - name: cache
      description: Provides the btdt cache.
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
      image: jgosmann/btdt:0.1
      workingDir: $(workspaces.cache.path)
      onError: continue
      script: |
        #!/bin/sh
        CACHE_KEY=node-modules-$(btdt hash package-lock.json)
        echo "Cache key: $CACHE_KEY"
        btdt restore --cache $(workspaces.cache.path) --keys $CACHE_KEY node_modules
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
      image: jgosmann/btdt:0.1
      workingDir: $(workspaces.git-sources.path)
      script: |
        #!/bin/sh
        if [ $(cat $(steps.step-restore-cache.exitCode.path)) -eq 0 ]; then
            echo "Cache restore succeeded, skipping cache store"
            exit 0
        fi
        CACHE_KEY=node-modules-$(btdt hash package-lock.json)
        echo "Cache key: $CACHE_KEY"
        btdt store --cache $(workspaces.cache.path) --keys $CACHE_KEY node_modules
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
      image: jgosmann/btdt:0.1
      workingDir: $(workspaces.git-sources.path)
      onError: continue
      script: |
        #!/bin/sh
        CACHE_KEY=node-modules-$(btdt hash package-lock.json)
        echo "Cache key: $CACHE_KEY"
        btdt restore --cache $(workspaces.cache.path) --keys $CACHE_KEY node_modules
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
        # run tests etc.
    - name: store-cache
      image: jgosmann/btdt
      workingDir: $(workspaces.git-sources.path)
      script: |
        #!/bin/sh
        if [ $(cat $(steps.step-restore-cache.exitCode.path)) -eq 0 ]; then
            echo "Cache restore succeeded, skipping cache store"
            exit 0
        fi
        CACHE_KEY=node-modules-$(btdt hash package-lock.json)
        echo "Cache key: $CACHE_KEY"
        btdt store --cache $(workspaces.cache.path) --keys $CACHE_KEY node_modules

  workspaces:
    - name: git-sources
      description: Provides the workspace with the cloned repository.
    - name: cache
      description: Provides btdt cache.
```

## Cleanup

To prevent the cache from growing indefinitely, you should configure a regular cleanup:

### Clean task

```yaml
# task_cache-clean.yaml
apiVersion: tekton.dev/v1beta1
kind: Task
metadata:
  name: cache-clean
spec:
  steps:
    - name: cache-clean
      image: jgosmann/btdt:0.1
      script: |
        #!/bin/sh
        btdt clean --cache $(workspaces.cache.path) --max-age 7d --max-size 10GiB
  workspaces:
    - name: cache
      description: Provides the btdt cache.
```

### Clean pipeline

```yaml
# pipeline_cache-clean.yaml
apiVersion: tekton.dev/v1beta1
kind: Pipeline
metadata:
  name: cache-clean-pipeline
spec:
  workspaces:
    - name: cache
  params:
    - name: runid
      type: string
  tasks:
    - name: cache-clean
      taskRef:
        name: cache-clean
        kind: Task
      workspaces:
        - name: cache
          workspace: cache
```

### Cron trigger

```yaml
# trigger_cache-clean.yaml
apiVersion: batch/v1
kind: CronJob
metadata:
  name: cache-clean-schedule
spec:
  schedule: '@hourly'
  jobTemplate:
    spec:
      template:
        spec:
          containers:
            - name: cache-clean-trigger
              image: curlimages/curl
              command: [ '/bin/sh', '-c' ]
              args: [ "curl --header \"Content-Type: application/json\" --data '{}' el-cache-clean-listener.default.svc.cluster.local:8080" ]
          restartPolicy: Never

---
apiVersion: triggers.tekton.dev/v1alpha1
kind: EventListener
metadata:
  name: cache-clean-listener
spec:
  triggers:
    - name: cache-clean-trigger
      interceptors: [ ]
      template:
        spec:
          resourcetemplates:
            - apiVersion: tekton.dev/v1beta1
              kind: PipelineRun
              metadata:
                name: cache-clean-$(uid)
              spec:
                pipelineRef:
                  name: cache-clean-pipeline
                params:
                  - name: runid
                    value: $(uid)
                workspaces:
                  - name: cache
                    persistentVolumeClaim:
                      claimName: my-tekton-cache
```
