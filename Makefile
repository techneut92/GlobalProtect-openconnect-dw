.SHELLFLAGS += -e

INCLUDE_GUI ?= 0
CARGO ?= cargo
DISABLE_RUST_TOOLCHAIN ?= 0
RUST_VERSION ?= 1.89
IGNORE_RUST_VERSION ?= 0

VERSION = $(shell grep '^version' Cargo.toml | head -1 | sed 's/version *= *"\(.*\)"/\1/')
REVISION ?= 1
RPM_SOURCE ?= %{name}.tar.gz

PPA_REVISION ?= 1
# Fork package name — kept distinct from upstream's `globalprotect-openconnect`
# (their PPA/AUR) so the two never collide. This package is the BACKEND only
# (gpservice/gpclient/gpauth); the GUI ships separately as a Flatpak.
PKG_NAME = globalprotect-openconnect-dw
PKG = $(PKG_NAME)-$(VERSION)
SERIES ?= $(shell lsb_release -cs)
PUBLISH ?= 0

export DEBEMAIL = dylanwestra@gmail.com
export DEBFULLNAME = Dylan Westra
export SNAPSHOT = $(shell test -f SNAPSHOT && echo "true" || echo "false")
export OFFLINE_BUILD = $(shell test -f OFFLINE_BUILD && echo "1" || echo "0")
# If OFFLINE is not set, use OFFLINE_BUILD
ifndef OFFLINE
	OFFLINE = $(OFFLINE_BUILD)
endif

CARGO_BUILD_ARGS = --release

ifeq ($(OFFLINE), 1)
	CARGO_BUILD_ARGS += --frozen
endif

ifeq ($(IGNORE_RUST_VERSION), 1)
	CARGO_BUILD_ARGS += --ignore-rust-version
endif

default: build

version:
	@echo $(VERSION)

clean-tarball:
	rm -rf .build/tarball
	rm -rf .vendor
	rm -rf vendor.tar.xz
	rm -rf .cargo

# Create a tarball, include the cargo dependencies if OFFLINE is set to 1
tarball: clean-tarball
	mkdir -p .cargo
	mkdir -p .build/tarball

	# If OFFLINE is set to 1, vendor all cargo dependencies
	# Generate a OFFLINE_BUILD file to indicate offline build
	if [ $(OFFLINE) -eq 1 ]; then \
		$(CARGO) vendor .vendor > .cargo/config.toml; \
		tar -cJf vendor.tar.xz .vendor; \
		touch OFFLINE_BUILD; \
	fi

	@echo "Creating tarball..."
	tar --exclude .vendor --exclude target --transform 's,^,${PKG}/,' -czf .build/tarball/${PKG}.tar.gz * .cargo

build: build-rs

build-rs:
	if [ $(OFFLINE) -eq 1 ]; then \
		tar -xJf vendor.tar.xz; \
	fi

	# Remove the rust-toolchain.toml if DISABLE_RUST_TOOLCHAIN is set to 1
	if [ $(DISABLE_RUST_TOOLCHAIN) -eq 1 ]; then \
		rm -vf rust-toolchain.toml; \
	fi

	# gpservice (daemon) + gpclient (CLI) stay lean and webkit-free.
	$(CARGO) build $(CARGO_BUILD_ARGS) -p gpclient -p gpservice --no-default-features
	# gpauth is built WITH its default features (webview-auth) so the GUI's
	# embedded-webview SSO works; it links webkit (hence the backend's webkit dep).
	$(CARGO) build $(CARGO_BUILD_ARGS) -p gpauth

	# Optional: build OUR Tauri GUI from source (apps/gpgui) for a native+GUI
	# package. This is never the upstream proprietary gpgui binary.
	if [ $(INCLUDE_GUI) -eq 1 ]; then \
		$(CARGO) build $(CARGO_BUILD_ARGS) -p gpgui; \
	fi

clean:
	$(CARGO) clean
	rm -rf .build
	rm -rf .vendor
	rm -rf apps/gpgui-helper/node_modules

install:
	@echo "Installing $(PKG_NAME) (backend)..."

	install -Dm755 target/release/gpclient $(DESTDIR)/usr/bin/gpclient
	install -Dm755 target/release/gpauth $(DESTDIR)/usr/bin/gpauth
	install -Dm755 target/release/gpservice $(DESTDIR)/usr/bin/gpservice

	# openconnect helper scripts
	install -Dm755 packaging/files/usr/libexec/gpclient/vpnc-script $(DESTDIR)/usr/libexec/gpclient/vpnc-script
	install -Dm755 packaging/files/usr/libexec/gpclient/hipreport.sh $(DESTDIR)/usr/libexec/gpclient/hipreport.sh

	# NetworkManager disconnect hooks
	install -Dm755 packaging/files/usr/lib/NetworkManager/dispatcher.d/pre-down.d/gpclient.down $(DESTDIR)/usr/lib/NetworkManager/dispatcher.d/pre-down.d/gpclient.down
	install -Dm755 packaging/files/usr/lib/NetworkManager/dispatcher.d/gpclient-nm-hook $(DESTDIR)/usr/lib/NetworkManager/dispatcher.d/gpclient-nm-hook

	# D-Bus system service (the host backend a Flatpak/native GUI talks to) + polkit:
	# the gpservice.manage action (D-Bus) and the passwordless pkexec rule (loopback).
	install -Dm644 packaging/files/usr/share/dbus-1/system-services/io.github.techneut92.GPService.service $(DESTDIR)/usr/share/dbus-1/system-services/io.github.techneut92.GPService.service
	install -Dm644 packaging/files/usr/share/dbus-1/system.d/io.github.techneut92.GPService.conf $(DESTDIR)/usr/share/dbus-1/system.d/io.github.techneut92.GPService.conf
	install -Dm644 packaging/files/usr/share/polkit-1/rules.d/49-gpservice.rules $(DESTDIR)/usr/share/polkit-1/rules.d/49-gpservice.rules
	install -Dm644 packaging/files/usr/share/polkit-1/actions/io.github.techneut92.gpservice.policy $(DESTDIR)/usr/share/polkit-1/actions/io.github.techneut92.gpservice.policy

	# Optional: install OUR Tauri GUI (built from source) for a native+GUI package.
	if [ $(INCLUDE_GUI) -eq 1 ]; then \
		install -Dm755 target/release/gpgui $(DESTDIR)/usr/bin/gpgui; \
		install -Dm644 apps/gpgui/packaging/io.github.techneut92.gpgui.desktop $(DESTDIR)/usr/share/applications/io.github.techneut92.gpgui.desktop; \
		install -Dm644 apps/gpgui/icons/128x128.png $(DESTDIR)/usr/share/icons/hicolor/128x128/apps/gpgui.png; \
	fi

uninstall:
	@echo "Uninstalling $(PKG_NAME)..."

	rm -f $(DESTDIR)/usr/bin/gpclient
	rm -f $(DESTDIR)/usr/bin/gpauth
	rm -f $(DESTDIR)/usr/bin/gpservice
	rm -f $(DESTDIR)/usr/bin/gpgui

	rm -f $(DESTDIR)/usr/libexec/gpclient/vpnc-script
	rm -f $(DESTDIR)/usr/libexec/gpclient/hipreport.sh

	rm -f $(DESTDIR)/usr/lib/NetworkManager/dispatcher.d/pre-down.d/gpclient.down
	rm -f $(DESTDIR)/usr/lib/NetworkManager/dispatcher.d/gpclient-nm-hook

	rm -f $(DESTDIR)/usr/share/dbus-1/system-services/io.github.techneut92.GPService.service
	rm -f $(DESTDIR)/usr/share/dbus-1/system.d/io.github.techneut92.GPService.conf
	rm -f $(DESTDIR)/usr/share/polkit-1/rules.d/49-gpservice.rules
	rm -f $(DESTDIR)/usr/share/polkit-1/actions/io.github.techneut92.gpservice.policy

	# Optional GUI (native+GUI package)
	rm -f $(DESTDIR)/usr/bin/gpgui
	rm -f $(DESTDIR)/usr/share/applications/io.github.techneut92.gpgui.desktop
	rm -f $(DESTDIR)/usr/share/icons/hicolor/128x128/apps/gpgui.png

clean-debian:
	rm -rf .build/deb

# Generate the debian package structure, without the changelog
init-debian: clean-debian tarball
	mkdir -p .build/deb
	cp .build/tarball/${PKG}.tar.gz .build/deb

	tar -xzf .build/deb/${PKG}.tar.gz -C .build/deb
	cd .build/deb/${PKG} && debmake

	cp -f packaging/deb/control.in .build/deb/$(PKG)/debian/control
	cp -f packaging/deb/rules.in .build/deb/$(PKG)/debian/rules
	cp -f packaging/deb/postrm .build/deb/$(PKG)/debian/postrm
	cp -f packaging/deb/compat .build/deb/$(PKG)/debian/compat

	# Split into backend + GUI binary packages (dh_install reads these globs)
	cp -f packaging/deb/globalprotect-openconnect-dw.install .build/deb/$(PKG)/debian/globalprotect-openconnect-dw.install
	cp -f packaging/deb/globalprotect-openconnect-dw-gui.install .build/deb/$(PKG)/debian/globalprotect-openconnect-dw-gui.install

	sed -i "s/@RUST_VERSION@/$(RUST_VERSION)/g" .build/deb/$(PKG)/debian/control

	sed -i "s/@RUST_VERSION@/$(RUST_VERSION)/g" .build/deb/$(PKG)/debian/rules

	rm -f .build/deb/$(PKG)/debian/changelog

deb: init-debian
	cd .build/deb/$(PKG) && dch --create --distribution unstable --package $(PKG_NAME) --newversion $(VERSION)-$(REVISION) "Bugfix and improvements."

	# Install build dependencies
	cd .build/deb/$(PKG) && sudo mk-build-deps --install --remove debian/control || echo "mk-build-deps failed, continuing"

	cd .build/deb/$(PKG) && debuild --preserve-env -e PATH -us -uc -b -d

check-ppa:
	if [ $(OFFLINE) -eq 0 ]; then \
		echo "Error: ppa build requires offline mode (OFFLINE=1)"; \
	fi

# Usage: make ppa SERIES=focal OFFLINE=1 PUBLISH=1
ppa: check-ppa init-debian
	$(eval SERIES_VER = $(shell distro-info --series $(SERIES) -r | cut -d' ' -f1))
	@echo "Building for $(SERIES) $(SERIES_VER)"

	rm -rf .build/deb/$(PKG)/debian/changelog
	cd .build/deb/$(PKG) && dch --create --distribution $(SERIES) --package $(PKG_NAME) --newversion $(VERSION)-$(REVISION)ppa$(PPA_REVISION)~ubuntu$(SERIES_VER) "Bugfix and improvements."

	cd .build/deb/$(PKG) && echo "y" | debuild -e PATH -S -sa -k"$(GPG_KEY_ID)" -p"gpg --batch --passphrase $(GPG_KEY_PASS) --pinentry-mode loopback"

	if [ $(PUBLISH) -eq 1 ]; then \
		cd .build/deb/$(PKG) && dput ppa:techneut92/globalprotect-openconnect-dw ../*.changes; \
	else \
		echo "Skipping ppa publish (PUBLISH=0)"; \
	fi

clean-rpm:
	rm -rf .build/rpm

# Generate RPM sepc file
init-rpm: clean-rpm
	mkdir -p .build/rpm

	cp packaging/rpm/globalprotect-openconnect.spec.in .build/rpm/globalprotect-openconnect.spec
	cp packaging/rpm/globalprotect-openconnect.changes.in .build/rpm/globalprotect-openconnect.changes

	sed -i "s/@VERSION@/$(VERSION)/g" .build/rpm/globalprotect-openconnect.spec
	sed -i "s/@REVISION@/$(REVISION)/g" .build/rpm/globalprotect-openconnect.spec
	sed -i "s|@SOURCE@|$(RPM_SOURCE)|g" .build/rpm/globalprotect-openconnect.spec
	sed -i "s/@DATE@/$(shell LC_ALL=en.US date "+%a %b %d %Y")/g" .build/rpm/globalprotect-openconnect.spec

	sed -i "s/@VERSION@/$(VERSION)/g" .build/rpm/globalprotect-openconnect.changes
	sed -i "s/@DATE@/$(shell LC_ALL=en.US date -u "+%a %b %e %T %Z %Y")/g" .build/rpm/globalprotect-openconnect.changes

rpm: init-rpm tarball
	rm -rf $(HOME)/rpmbuild
	rpmdev-setuptree

	cp .build/tarball/${PKG}.tar.gz $(HOME)/rpmbuild/SOURCES/${PKG_NAME}.tar.gz
	rpmbuild -ba .build/rpm/globalprotect-openconnect.spec

	# Copy RPM package from build directory
	cp $(HOME)/rpmbuild/RPMS/$(shell uname -m)/$(PKG_NAME)*.rpm .build/rpm

	# Copy the SRPM only for x86_64.
	if [ "$(shell uname -m)" = "x86_64" ]; then \
		cp $(HOME)/rpmbuild/SRPMS/$(PKG_NAME)*.rpm .build/rpm; \
	fi

clean-pkgbuild:
	rm -rf .build/pkgbuild

init-pkgbuild: clean-pkgbuild tarball
	mkdir -p .build/pkgbuild

	cp .build/tarball/${PKG}.tar.gz .build/pkgbuild
	cp packaging/pkgbuild/PKGBUILD.in .build/pkgbuild/PKGBUILD

	sed -i "s/@PKG_NAME@/$(PKG_NAME)/g" .build/pkgbuild/PKGBUILD
	sed -i "s/@VERSION@/$(VERSION)/g" .build/pkgbuild/PKGBUILD
	sed -i "s/@REVISION@/$(REVISION)/g" .build/pkgbuild/PKGBUILD

pkgbuild: init-pkgbuild
	cd .build/pkgbuild && makepkg -s --noconfirm

clean-apk:
	rm -rf .build/apk

init-apk: clean-apk tarball
	mkdir -p .build/apk

	cp .build/tarball/${PKG}.tar.gz .build/apk
	cp packaging/apk/APKBUILD.in .build/apk/APKBUILD

	sed -i "s/@PKG_NAME@/$(PKG_NAME)/g" .build/apk/APKBUILD
	sed -i "s/@VERSION@/$(VERSION)/g" .build/apk/APKBUILD
	sed -i "s/@REVISION@/$(REVISION)/g" .build/apk/APKBUILD
	checksum=$$(sha512sum .build/apk/${PKG}.tar.gz | cut -d' ' -f1); \
		sed -i "s/@SHA512@/$$checksum/g" .build/apk/APKBUILD

apk: init-apk
	cd .build/apk && abuild -r -P "$(CURDIR)/.build/apk/packages"

	find .build/apk/packages -type f -name "$(PKG_NAME)-*.apk" -exec cp {} .build/apk \;

clean-binary:
	rm -rf .build/binary

binary: clean-binary tarball
	mkdir -p .build/binary

	cp .build/tarball/${PKG}.tar.gz .build/binary
	tar -xzf .build/binary/${PKG}.tar.gz -C .build/binary

	mkdir -p .build/binary/$(PKG_NAME)_$(VERSION)/artifacts

	make -C .build/binary/${PKG} build INCLUDE_GUI=$(INCLUDE_GUI)
	make -C .build/binary/${PKG} install DESTDIR=$(PWD)/.build/binary/$(PKG_NAME)_$(VERSION)/artifacts

	cp packaging/binary/Makefile.in .build/binary/$(PKG_NAME)_$(VERSION)/Makefile

	# Create a tarball for the binary package
	tar -cJf .build/binary/$(PKG_NAME)_$(VERSION)_$(shell uname -m).bin.tar.xz -C .build/binary $(PKG_NAME)_$(VERSION)

	# Generate sha256sum
	cd .build/binary && sha256sum $(PKG_NAME)_$(VERSION)_$(shell uname -m).bin.tar.xz | cut -d' ' -f1 > $(PKG_NAME)_$(VERSION)_$(shell uname -m).bin.tar.xz.sha256
