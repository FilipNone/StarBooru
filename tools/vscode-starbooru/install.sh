#!/usr/bin/env bash
# Packages the StarBooru Runner as a .vsix (no npm/vsce needed) and installs it.
# Usage: tools/vscode-starbooru/install.sh [code|codium|code-oss]
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
code_cli="${1:-code}"
version="$(sed -n 's/^  "version": "\(.*\)",$/\1/p' "$here/package.json")"
out="$here/starbooru-runner-$version.vsix"

python3 -I - "$here" "$out" "$version" <<'PY'
import sys, zipfile, pathlib
src, out, version = pathlib.Path(sys.argv[1]), sys.argv[2], sys.argv[3]
files = ["package.json", "extension.js", "README.md", "media/star.svg"]
manifest = f"""<?xml version="1.0" encoding="utf-8"?>
<PackageManifest Version="2.0.0" xmlns="http://schemas.microsoft.com/developer/vsx-schema/2011">
  <Metadata>
    <Identity Language="en-US" Id="starbooru-runner" Version="{version}" Publisher="starbooru"/>
    <DisplayName>StarBooru Runner</DisplayName>
    <Description xml:space="preserve">Play buttons for the StarBooru apps.</Description>
    <Properties>
      <Property Id="Microsoft.VisualStudio.Code.Engine" Value="^1.85.0"/>
    </Properties>
  </Metadata>
  <Installation><InstallationTarget Id="Microsoft.VisualStudio.Code"/></Installation>
  <Dependencies/>
  <Assets>
    <Asset Type="Microsoft.VisualStudio.Code.Manifest" Path="extension/package.json" Addressable="true"/>
  </Assets>
</PackageManifest>
"""
types = """<?xml version="1.0" encoding="utf-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension=".json" ContentType="application/json"/>
  <Default Extension=".js" ContentType="application/javascript"/>
  <Default Extension=".md" ContentType="text/markdown"/>
  <Default Extension=".svg" ContentType="image/svg+xml"/>
  <Default Extension=".vsixmanifest" ContentType="text/xml"/>
</Types>
"""
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    z.writestr("[Content_Types].xml", types)
    z.writestr("extension.vsixmanifest", manifest)
    for f in files:
        z.write(src / f, f"extension/{f}")
PY

"$code_cli" --install-extension "$out" --force
echo "Installed $out. Reload VS Code (Developer: Reload Window) and click the star in the activity bar."
