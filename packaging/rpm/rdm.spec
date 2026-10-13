Name:           rdm
Version:        0.1.0
Release:        1%{?dist}
Summary:        Fast, resumable, multi-connection download manager
License:        MIT
URL:            https://github.com/mehrabix/rdm
Source0:        https://github.com/mehrabix/rdm/releases/download/v%{version}/rdm-linux-x86_64.tar.gz
# Copy assets/icon-256.png into the rpmbuild SOURCES dir as rdm.png before building.
Source1:        rdm.png
Requires:       webkit2gtk4.1
Requires:       gtk3
Requires:       libayatana-appindicator-gtk3
Requires:       librsvg2

%global debug_package %{nil}

%description
RDM (Rust Download Manager) splits a file into parallel HTTP range requests,
resumes after interruption, and runs native on Linux, Windows and macOS.

%prep
%setup -q -c -T
tar -xzf %{SOURCE0}

%build
# nothing to build — this packages the published release

%install
BIN="$(find . -type f -name rdm | head -1)"
install -Dm755 "$BIN" %{buildroot}%{_libexecdir}/rdm/rdm

mkdir -p %{buildroot}%{_bindir}
printf '#!/bin/sh\nexec %{_libexecdir}/rdm/rdm "$@"\n' > %{buildroot}%{_bindir}/rdm
chmod 755 %{buildroot}%{_bindir}/rdm

install -Dm644 %{SOURCE1} %{buildroot}%{_datadir}/icons/hicolor/256x256/apps/rdm.png

install -d %{buildroot}%{_datadir}/applications
cat > %{buildroot}%{_datadir}/applications/rdm.desktop <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=RDM
GenericName=Download Manager
Comment=Rust Download Manager
Exec=rdm
Icon=rdm
Terminal=false
Categories=Network;FileTransfer;
DESKTOP

%files
%{_bindir}/rdm
%{_libexecdir}/rdm/rdm
%{_datadir}/applications/rdm.desktop
%{_datadir}/icons/hicolor/256x256/apps/rdm.png

%changelog
* Tue Oct 13 2026 mehrabix <mehrabi@post.com> - 0.1.0-1
- Initial package
