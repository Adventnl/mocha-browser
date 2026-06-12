# Resource Broker Prototype (Milestone 18)

M18 uses a prepared-document model instead of a full asynchronous resource
broker.

Current behavior:

- the browser/preparer owns privileged file or network loading;
- the sandboxed renderer receives `PreparedDocument { final_url, html }`;
- the renderer can render that prepared HTML through `RenderPreparedDocument`;
- direct `RenderDocument` loads are rejected after a restricted sandbox policy is
  applied;
- external subresources are not brokered in M18's restricted path.

This means M18 proves the capability boundary, but it does not yet support a full
browser resource pipeline in the sandboxed renderer. Stylesheets can be present
as inline `<style>` in prepared HTML. External CSS, images, and other resources
need a future broker/resource-map design before they work in the restricted path.
