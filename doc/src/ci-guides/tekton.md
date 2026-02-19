# Tekton

There are two general approaches to use `btdt` with Tekton:

- Using `btdt-server` to store the cache. This approach is described in
  the [Tekton with btdt-server](./tekton-server.md) guide.
- Using a persistent volume claim (PVC) to store the cache. This approach is described in
  the [Tekton with PVC](./tekton-pvc.md) guide.

The `btdt-server` approach is generally more flexible and easier to set up, but requires to deploy the `btdt-server`
component in your Kubernetes cluster and creation of an authentication token.

The PVC approach does not require any additional components, but is subject to Tekton's limitations regarding the usage
of PVCs. By default, only a single PVC can be mounted into a task, so you will have to use the same PVC for your source
code and the cache. This also means that you cannot use a
[`volumeClaimTemplate`](https://tekton.dev/docs/pipelines/workspaces/#volumeclaimtemplate) that is creates a fresh PVC
for each pipeline run. Alternatively, you can disable
the [affinity assistant](https://tekton.dev/docs/pipelines/affinityassistants/) to be able to mount multiple PVCs into a
task.
Run `kubectl edit configmap feature-flags` to edit the configuration.
