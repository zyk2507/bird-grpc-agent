#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
VERSION="${VERSION:-${1:-}}"
if [ -z "$VERSION" ]; then
  echo "Missing version. Set VERSION or pass it as the first argument." >&2
  exit 1
fi
VERSION="${VERSION#v}"
DIST_DIR="$ROOT_DIR/dist"
STAGE_DIR="$DIST_DIR/stage"

rm -rf "$DIST_DIR"
mkdir -p "$STAGE_DIR"

cd "$ROOT_DIR"

cargo build --release

install -d "$STAGE_DIR/usr/sbin"
install -m 0755 "$ROOT_DIR/target/release/bird-grpc-agent" "$STAGE_DIR/usr/sbin/bird-grpc-agent"

install -d "$STAGE_DIR/etc/bird-grpc-agent"
install -m 0644 "$ROOT_DIR/packaging/config/agent.env" "$STAGE_DIR/etc/bird-grpc-agent/agent.env"

install -d "$STAGE_DIR/lib/systemd/system"
install -m 0644 "$ROOT_DIR/packaging/systemd/bird-grpc-agent.service" "$STAGE_DIR/lib/systemd/system/bird-grpc-agent.service"

# Raw binary artifact
install -d "$DIST_DIR/bin"
install -m 0755 "$ROOT_DIR/target/release/bird-grpc-agent" "$DIST_DIR/bin/bird-grpc-agent"

# Build .deb
DEB_DIR="$DIST_DIR/deb"
mkdir -p "$DEB_DIR/DEBIAN"
cat > "$DEB_DIR/DEBIAN/control" <<CONTROL
Package: bird-grpc-agent
Version: $VERSION
Section: net
Priority: optional
Architecture: amd64
Maintainer: bird-ci <ci@example.invalid>
Description: BIRD gRPC exporter
CONTROL

cp -a "$STAGE_DIR"/* "$DEB_DIR"/

dpkg-deb --build "$DEB_DIR" "$DIST_DIR/bird-grpc-agent-${VERSION}-amd64.deb"

# Build .rpm
RPM_TOP="$DIST_DIR/rpmbuild"
RPM_SPEC="$RPM_TOP/SPECS/bird-grpc-agent.spec"
RPM_FILELIST="$RPM_TOP/SPECS/filelist"
mkdir -p "$RPM_TOP"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}

find "$STAGE_DIR" -mindepth 1 -printf "/%P\n" | sort -u > "$RPM_FILELIST"

cat > "$RPM_SPEC" <<SPEC
Name: bird-grpc-agent
Version: $VERSION
Release: 1%{?dist}
Summary: BIRD gRPC exporter
License: MIT
BuildArch: x86_64

%description
BIRD gRPC exporter.

%prep

%build

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}
cp -a "$STAGE_DIR"/* %{buildroot}/

%files -f $RPM_FILELIST

%post
systemctl daemon-reload >/dev/null 2>&1 || true

%preun
if [ $1 -eq 0 ]; then
  systemctl daemon-reload >/dev/null 2>&1 || true
fi
SPEC

rpmbuild -bb "$RPM_SPEC" --define "_topdir $RPM_TOP" --define "_rpmdir $DIST_DIR" >/dev/null

RPM_OUTPUTS=$(find "$DIST_DIR" -type f -name "*.rpm")
if [ -z "$RPM_OUTPUTS" ]; then
  echo "No RPMs found in $DIST_DIR" >&2
  exit 1
fi

for rpm in $RPM_OUTPUTS; do
  if [ "$(dirname "$rpm")" != "$DIST_DIR" ]; then
    cp -a "$rpm" "$DIST_DIR"/
  fi
done
