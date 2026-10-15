# Mandatory rules for every Forge port

## One authentication per operation

This is a hard product requirement from the project owner, applicable to every
distribution and packaging format.

- A complete installation or preparation invocation must require at most one
  privilege authentication in total. Count sudo and Polkit together, not separately.
- `forge prepare --all` must not ask again for each image, VM, download, storage
  step or libvirt call. The number of images must not determine the number of prompts.
- Group privileged work into a bounded authenticated operation. Long downloads
  must not lead to a new authentication prompt for each subsequent image.
- Cancellation or authentication failure must stop the privileged operation
  cleanly rather than repeatedly presenting prompts.
- Preserve privilege boundaries, image verification, ownership checks and host
  security policy. Do not achieve this by disabling Polkit, adding unrestricted
  passwordless sudo, running all of Forge as root or weakening SELinux/AppArmor.

## Acceptance and reporting

Verify each port with cold credentials, the full multi-image preparation path,
and a download long enough to exceed an ordinary credential timeout. Count all
authentication dialogs/prompts and check cancellation and failure handling.
Cover libvirt authorization as well as storage and hook operations.

A port that prompts per image violates this requirement, even if it eventually
prepares the images correctly. Do not mark it fully ready until this is fixed and
verified. Commit and PR descriptions for port work must explicitly report the
authentication behavior and whether this acceptance check passed, failed or
remains unverified. Documenting this rule alone does not implement the fix.
