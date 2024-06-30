# `fukomaster`

Simulate a trackpad with your physical mouse.

<center>

<video width="640" height="360" controls>
  <source src="assets/demo.mp4" type="video/mp4">
</video>

*Using fukomaster with an MX Master 3S on GNOME with the
[PaperWM](https://github.com/paperwm/PaperWM) tiling WM extension*

</center>

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

fukomaster allows you to simulate this behavior by temporarily turning your mouse into a trackpad;
starting a three-finger swipe (or however many you have configured); then converting your mouse
movements into finger movements on the virtual trackpad.

Note that this doesn't just work with switching workspaces - you can use trackpad gestures for all
kinds of things, such as swiping right to go back in a web browser. You are, however, limited to the
fact that all of your virtual fingers will be moving in the same direction at the same speed, so you
can't, for example, simulate a pinch-and-zoom.

## Usage

Tested on:
- [x] Wayland
  - [x] GNOME 46
- [ ] Xorg
  - TODO

This tool must be run as root, since it needs to read raw mouse inputs from your physical mouse.

**If using [Solaar],** the *Key/Button Diversion* for the *Mouse Gesture Button* must be set to
*Regular*, so that this tool can read the mouse gesture button being pressed/released.

This tool uses `evdev`'s grab functionality, which allows a process to temporarily lock a device's
inputs so that only that process can consume them, and other processes do not read the events.
**This may cause some issues with other processes which also grab!**

This tool is very customizable - see the `--help` for all the command line flags.

### Packages

TODO

### Compile from source

```bash
cargo run
```

## Etymology

- `master`: because I wrote it for my MX Master 3S
- `fuko`: ヒトデです！

[MX Master 3S]: https://www.logitech.com/en-eu/products/mice/mx-master-3s.910-006559.html
[Solaar]: https://pwr-solaar.github.io/Solaar/
