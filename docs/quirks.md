## Quirks

While Determinate Nix Installer tries to provide a comprehensive and unquirky experience, there are unfortunately some issues that may require manual intervention or operator choices.

### Using MacOS after removing Nix while nix-darwin was still installed, network requests fail

If Nix was previously uninstalled without uninstalling [nix-darwin] first, you may experience errors similar to this:

```shell
nix shell nixpkgs#curl

error: unable to download 'https://cache.nixos.org/g8bqlgmpa4yg601w561qy2n576i6g0vh.narinfo': Problem with the SSL CA cert (path? access rights?) (77)
```

This occurs because `nix-darwin` provisions an `org.nixos.activate-system` service which remains after Nix is uninstalled.
The `org.nixos.activate-system` service in this state interacts with the newly installed Nix and changes the SSL certificates it uses to be a broken symlink.

```shell
ls -lah /etc/ssl/certs

total 0
drwxr-xr-x  3 root  wheel    96B Oct 17 08:26 .
drwxr-xr-x  6 root  wheel   192B Sep 16 06:28 ..
lrwxr-xr-x  1 root  wheel    41B Oct 17 08:26 ca-certificates.crt -> /etc/static/ssl/certs/ca-certificates.crt
```

The problem is compounded by the matter that the [`nix-darwin` uninstaller][uninstalling] will not work after uninstalling Nix, since it uses Nix and requires network connectivity.

It's possible to resolve this situation by removing the `org.nixos.activate-system` service and the `ca-certificates`:

```shell
sudo rm /Library/LaunchDaemons/org.nixos.activate-system.plist
sudo launchctl bootout system/org.nixos.activate-system
/nix/nix-installer uninstall
sudo rm /etc/ssl/certs/ca-certificates.crt
```

Run the installer again and it should work.

Up-to-date versions of the installer will refuse to uninstall until [nix-darwin] is uninstalled first, helping to mitigate this problem.

[nix-darwin]: https://github.com/LnL7/nix-darwin
[uninstalling]: https://github.com/LnL7/nix-darwin#uninstalling
