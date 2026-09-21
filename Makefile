# The gate is `make check`. It is what CI runs and all it runs, so the two
# cannot drift. Every command in it exits zero, the formatter lists no files,
# and the tests pass.
#
# The order is what fails fastest first: formatting costs nothing, a build error
# is the commonest mistake and the cheapest to find, and the tests are last
# because they are the slowest.

APP := .build/otransit.app
SHOTS := shots

.PHONY: help
help:
	@echo "make check    the gate: format, build, test"
	@echo "make format   rewrite the sources the formatter would change"
	@echo "make app      build the release bundle at $(APP)"
	@echo "make run      build the bundle and launch it"
	@echo "make shots    photograph every screen into $(SHOTS)/"
	@echo "make clean    remove everything built"

.PHONY: check
check:
	swift format lint --strict --recursive Sources Tests
	swift build
	swift build --build-tests
	swift test

.PHONY: format
format:
	swift format --in-place --recursive Sources Tests

# An app bundle, because a menu bar app is one. A bare executable has no
# Info.plist, so it gets a Dock icon, a menu bar of its own, and no way to say
# it wants neither.
.PHONY: app
app:
	swift build --configuration release
	rm -rf $(APP)
	mkdir -p $(APP)/Contents/MacOS $(APP)/Contents/Resources
	cp .build/release/OTransit $(APP)/Contents/MacOS/otransit
	cp Resources/Info.plist $(APP)/Contents/Info.plist
	# Signed to run on this machine and no other. A release for anyone else
	# needs a Developer ID and notarising, which is a decision not yet made.
	codesign --force --sign - $(APP)

# Every screen as a PNG, drawn by the app into its own window. See Shot.swift:
# a menu bar popover cannot be opened from a script, and this is the nearest
# honest equivalent to the replay shell the Rust and Go ports check screens with.
# `browse` needs a cache on disk; the rest are states asked for directly.
.PHONY: shots
shots: app
	$(APP)/Contents/MacOS/otransit shot --into $(SHOTS)

.PHONY: run
run: app
	open $(APP)

.PHONY: clean
clean:
	rm -rf .build $(SHOTS)
