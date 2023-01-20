##### Description

<!---
Please include a short description of what your PR does and / or the motivation behind it
--->

##### Checklist

- [ ] Formatted with `cargo fmt`
- [ ] Built with `nix build`
- [ ] Ran flake checks with `nix flake check`
- [ ] Added or updated relevant tests (leave unchecked if not applicable)
- [ ] Added or updated relevant documentation (leave unchecked if not applicable)
- [ ] Linked to related issues (leave unchecked if not applicable)

##### Validating with `install.determinate.systems`

If a maintainer has added the `upload to s3` label to this PR, it will become available for installation via `install.determinate.systems`:

```shell
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix/pr/$PR_NUMBER | sh -s -- install
```
