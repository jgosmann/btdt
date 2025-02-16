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

## Provide the cache workspace to the task

tbd

## Restore the cache

tbd

## Install dependencies only on cache miss

tbd

## Store the cache

tbd

## Example of complete task