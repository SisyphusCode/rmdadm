PREFIX ?= /usr/local
BINDIR ?= $(PREFIX)/sbin
SYSTEMDDIR ?= /etc/systemd/system
UDEVDIR ?= /etc/udev/rules.d

all: build

build:
	cargo build --release

install: build
	install -d $(DESTDIR)$(BINDIR)
	install -m 755 target/release/rmdadm $(DESTDIR)$(BINDIR)/rmdadm
	
	install -d $(DESTDIR)$(SYSTEMDDIR)
	install -m 644 systemd/rmdadm.service $(DESTDIR)$(SYSTEMDDIR)/
	install -m 644 systemd/rmdadm-scrub.service $(DESTDIR)$(SYSTEMDDIR)/
	install -m 644 systemd/rmdadm-scrub.timer $(DESTDIR)$(SYSTEMDDIR)/
	
	install -d $(DESTDIR)$(UDEVDIR)
	install -m 644 udev/64-rmdadm.rules $(DESTDIR)$(UDEVDIR)/

clean:
	cargo clean

.PHONY: all build install clean
