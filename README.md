# `fukomaster`

Emulate a trackpad with your physical mouse.

## Motivation

Mice like the [MX Master 3S] have a dedicated gesture button which can be used to activate desktop
actions. On Windows, this allows you to do useful functions like switch between workspaces with
a flick of the mouse. However, Linux has much worse support for these gestures.

Tools like [Solaar] allow you to assign actions to the gesture button using rules, which you can use
to switch workspaces. For example, a rule like "on mouse gesture right -> press keybind
`Super+Right`" will press the `Super+Right` keybind when you perform a right swipe, and if your WM
is configured to interpret `Super+Right` as "switch to the next workspace", this will change the
workspace.

However, WMs like KDE's `kwin` and GNOME's `mutter` have an even nicer tool for switching workspaces
in this gesture manner: the three-finger swipe on the trackpad. If you press down on a trackpad with
three fingers and start swiping left/right, the desktop will swipe **smoothly** in the direction
that you swipe - that is, it's not an abrupt change like a keybinding, but a gradual process. The
more you swipe, or the faster you swipe, the further the desktop moves to the next workspace.

fukomaster allows you to emulate this behavior by temporarily turning your mouse into a trackpad;
starting a three-finger swipe (or however many you have configured); then converting your mouse
movements into finger movements on the virtual trackpad.

[MX Master 3S]: https://www.logitech.com/en-eu/products/mice/mx-master-3s.910-006559.html
[Solaar]: https://pwr-solaar.github.io/Solaar/

## Demo

TODO videos

## Usage

Tested on:
- [x] Wayland
  - [x] GNOME 46
- [ ] X
  - TODO

### Packages

TODO

### Compile from source

```bash
cargo run
```

## Etymology

- `master`: because I wrote it for my MX Master 3S
- `fuko`: iykyk :)
