param([switch]$Systemd = $false)
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"


# 22.04 https://cloud-images.ubuntu.com/wsl/jammy/current/
$url = "https://cloud-images.ubuntu.com/wsl/jammy/current/ubuntu-jammy-wsl-amd64-wsl.rootfs.tar.gz"
$File = "ubuntu-jammy-wsl-amd64-wsl.rootfs.tar.gz"
$Name = "ubuntu-jammy"


$TemporaryDirectory = "$HOME/nix-installer-wsl-tests-temp"
$Image = "$TemporaryDirectory\$File"
if (!(Test-Path -Path $Image)) {
    Write-Output "Fetching $File to $Image..."
    New-Item $TemporaryDirectory -ItemType Directory | Out-Null
    Invoke-WebRequest -Uri "https://cloud-images.ubuntu.com/wsl/jammy/current/ubuntu-jammy-wsl-amd64-wsl.rootfs.tar.gz" -OutFile $Image
} else {
    Write-Output "Found existing $Image..."
}

$DistroName = "nix-installer-test-$Name"
$InstallRoot = "$TemporaryDirectory\wsl-$Name"
Write-Output "Creating WSL distribution $DistroName from $Image at $InstallRoot..."
wsl --import $DistroName $InstallRoot $Image
if ($LastExitCode -ne 0) {
    exit $LastExitCode
}

Write-Output "Preparing $DistroName for nix-installer..."
wsl --distribution $DistroName bash --login -c "apt update --quiet"
if ($LastExitCode -ne 0) {
    exit $LastExitCode
}
wsl --distribution $DistroName bash --login -c "apt install --quiet --yes curl build-essential"
if ($LastExitCode -ne 0) {
    exit $LastExitCode
}
wsl --distribution $DistroName bash --login -c "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --quiet"
if ($LastExitCode -ne 0) {
    exit $LastExitCode
}

if ($Systemd) {
    $wslConf = "[boot]`nsystemd=true"
    New-Item -Path "\\wsl$\$DistroName\etc\wsl.conf" -ItemType "file" -Value $wslConf
    wsl --shutdown
    if ($LastExitCode -ne 0) {
        exit $LastExitCode
    }
}

Write-Output "Building and runnings nix-installer in $DistroName..."
Copy-Item -Recurse "$PSScriptRoot\..\.." -Destination "\\wsl$\$DistroName\nix-installer"
$MaybeInitChoice = switch ($Systemd) {
    $true { "" }
    $false { "--init none" }
}
wsl --distribution $DistroName bash --login -c "/root/.cargo/bin/cargo run --quiet --manifest-path /nix-installer/Cargo.toml -- install linux --no-confirm $MaybeInitChoice"
if ($LastExitCode -ne 0) {
    exit $LastExitCode
}

Write-Output "Testing installed Nix on $DistroName..."
wsl --distribution $DistroName bash --login -c "nix run nixpkgs#hello"
if ($LastExitCode -ne 0) {
    exit $LastExitCode
}

Write-Output "Unregistering $DistroName and removing $InstallRoot..."
wsl --unregister $DistroName
if ($LastExitCode -ne 0) {
    exit $LastExitCode
}
Remove-Item $InstallRoot
