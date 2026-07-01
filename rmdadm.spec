Name:           rmdadm
Version:        0.1.0
Release:        9%{?dist}
Summary:        A modern Rust rewrite of mdadm
License:        MIT
URL:            https://github.com/SisyphusCode/rmdadm
Source0:        %{name}-%{version}.tar.gz

%global debug_package %{nil}

BuildRequires:  make
BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  systemd-rpm-macros
BuildRequires:  openssl-devel

%description
A modern Rust rewrite of mdadm

%prep
%setup -q

%build
make build

%install
make install DESTDIR=%{buildroot} PREFIX=/usr BINDIR=/usr/sbin SYSTEMDDIR=%{_unitdir} UDEVDIR=/usr/lib/udev/rules.d

%files
/usr/sbin/rmdadm
%{_unitdir}/rmdadm.service
%{_unitdir}/rmdadm-scrub.service
%{_unitdir}/rmdadm-scrub.timer
/usr/lib/udev/rules.d/64-rmdadm.rules

%changelog
* Tue Jun 30 2026 Sisyphus <sisyphus@example.com> - 0.1.0-9
- Add operational migration, cluster, SMART health, and BTRFS CLI/API surfaces
- Add Kubernetes CRD/RBAC manifests and fix container daemon startup
- Implement assemble --auto and extend OpenAPI coverage

* Tue Jun 30 2026 Sisyphus <sisyphus@example.com> - 0.1.0-8
- Fix v1.x superblock layout, checksum, and chunk-size sector encoding
- Add library target so API integration tests compile and pass
- Align CLI/API chunk-size handling and stabilize API tests

* Tue Jun 30 2026 Sisyphus <sisyphus@example.com> - 0.1.0-7
- Fix MD ioctl ABI bindings and RUN_ARRAY invocation
- Ensure create uses an actual MD block device before issuing ioctls
- Reassemble newly written v1.x superblocks through the normal MD ioctl flow

* Tue Jun 30 2026 Sisyphus <sisyphus@example.com> - 0.1.0-6
- Fix device number extraction in RAID array creation
- Correct MduDiskInfo field initialization
- Skip SET_ARRAY_INFO for v1.x metadata

* Tue Jun 30 2026 Sisyphus <sisyphus@example.com> - 0.1.0-1
- Initial package
