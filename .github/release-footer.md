
## Downloads

| | |
| --- | --- |
| **Windows** (10 or 11, 64-bit) | `eppi-*-windows-x64.zip`: unzip it anywhere and run `Eppi.exe`. |
| **macOS** (11 or newer, Apple Silicon or Intel) | `eppi-*-macos.zip`: unzip it and move `Eppi.app` to Applications. |
| **Linux** (x86-64) | `eppi-*-linux-x64.tar.gz`: unpack it and run `./install.sh` inside, or just `./eppi`. |

The builds aren't signed with a paid certificate, so the first launch needs one extra step:

- **Windows**: SmartScreen may say "Windows protected your PC". Click **More info**, then **Run anyway**.
- **macOS**: if macOS says it can't check Eppi for malware, open **System Settings > Privacy & Security** and click **Open Anyway** next to Eppi. Or run `xattr -dr com.apple.quarantine /Applications/Eppi.app` once.

`SHA256SUMS.txt` has each file's checksum. Every file also has a build attestation, which proves this repo's release workflow built it from this tag:

```sh
gh attestation verify eppi-*-windows-x64.zip --repo rj-jones/eppi
```
