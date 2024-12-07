# Troubleshooting

- [Your system can't find Nix](#your-system-cant-find-nix)

## Your system can't find Nix

### Issue

You've run the installer but when you run any Nix command, like `nix --version`, and Nix isn't found:

```shell
$ nix --version
bash: nix: command not found
```

### Likely problem

Nix isn't currently on your `PATH`.

### Potential solutions

1. Initialize your Nix profile:

   ```shell
   . /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh
   ```

   This script sets up various environment variables that Nix needs to work.
   The installer does prompt you to run this command when it's finished with installation but it's easy to miss or forget.

2. Ensure that you're not overriding your existing `PATH` somewhere.
   If you have a `bash_profile`, `zshrc`, or other file that modifies your `PATH`, make sure that it _appends_ to your `PATH` rather than setting it directly.

   ```bash
   # Do this ✅
   PATH=$PATH${PATH:+:}path1:path2:path3

   # Not this ❌
   PATH=path1:path2:path3
   ```
