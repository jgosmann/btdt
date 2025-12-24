# Jenkins

This guide explains how to use `btdt` in a [Jenkins](https://www.jenkins.io/) pipeline.

You can either use a cache local to the Jenkins agents (but it won't be shared between different agents),
or a remote cache using a `btdt-server` instance.

## Agent-local cache

For an agent-local setup, you only need to [install the `btdt` CLI](../install.md) on your Jenkins agents.
Then you can integrate `btdt` commands into your pipeline script as described
in [Getting Started](../getting_started.md).
Ensure that the cache path you use is writable by the Jenkins agent user.

An example `Jenkinsfile` could look like this:

```groovy
#!groovy

pipeline {
    agent any

    stages {
        stage('Install dependencies') {
            steps {
                sh '''
                    CACHE_PATH=/var/lib/btdt/cache  # Path to cache on the Jenkins agent
                    CACHE_KEY=cache-key-$(btdt hash package-lock.json)
                    btdt restore --cache "$CACHE_PATH" --keys $CACHE_KEY node_modules
                    RESTORE_EXIT_CODE=$?
                    if [ $RESTORE_EXIT_CODE -ne 0 ]; then
                        npm ci  # Install dependencies
                        btdt store --cache "$CACHE_PATH" --keys $CACHE_KEY node_modules
                    fi
                '''
            }
        }
        stage('Run tests') {
            steps {
                // ...
            }
        }
    }
}
```

### Cleanup

To prevent the local cache from growing indefinitely, you can set up a periodic cleanup job in Jenkins.
For each agent, create a new Jenkins pipeline job that runs periodically (e.g., daily or weekly)
and add a stage that runs the `btdt cleanup` command on the cache path.
For example:

```groovy
#!groovy
pipeline {
    agent { node { label 'your-agent-label' } }

    triggers {
        cron('H H * * 0')  // Run weekly on Sundays
    }

    stages {
        stage('Cleanup cache') {
            steps {
                sh '''
                    CACHE_PATH=/var/lib/btdt/cache  # Path to cache on the Jenkins agent
                    btdt cleanup --cache "$CACHE_PATH" --max-age 7d --max-size 10G
                '''
            }
        }
    }
}
```

## Remote cache with btdt-server

The setup with a remote cache is very similar to the agent-local setup.
You also need to [install the `btdt` CLI](../install.md) on your Jenkins agents.
In addition, you need to [have a `btdt-server` instance running somewhere](../btdt-server/deployment.md)
and [an authorization token generated](../btdt-server/authorization.md).

Provide the authorization token as "secret file" credential in Jenkins ("Manage Jenkins" → "Credentials").

Then you can integrate `btdt` commands into your pipeline script as described
in [Getting Started](../getting_started.md),
using the remote cache URL and an authentication token file provided from the Jenkins credential.

An example `Jenkinsfile` could look like this:

```groovy
#!groovy

pipeline {
    agent any

    stages {
        stage('Install dependencies') {
            steps {
                script {
                    withCredentials([
                            file(credentialsId: 'btdt-auth-token', variable: 'BTDT_AUTH_TOKEN_FILE'),
                    ]) {
                        sh '''
                            CACHE_URL=http://btdt.example.com:8707/api/caches/my-cache
                            CACHE_KEY=cache-key-$(btdt hash package-lock.json)
                            btdt restore \\
                              --cache "$CACHE_URL" \\
                              --auth-token-file "$BTDT_AUTH_TOKEN_FILE" \\
                              --keys $CACHE_KEY \\
                              node_modules
                            RESTORE_EXIT_CODE=$?
                            if [ $RESTORE_EXIT_CODE -ne 0 ]; then
                                npm ci  # Install dependencies
                                btdt store \\
                                  --cache "$CACHE_URL" \\
                                  --auth-token-file "$BTDT_AUTH_TOKEN_FILE" \\
                                  --keys $CACHE_KEY \\
                                  node_modules
                            fi
                        '''
                    }
                }
            }
            stage('Run tests') {
                steps {
                    // ...
                }
            }
        }
    }
}
```
