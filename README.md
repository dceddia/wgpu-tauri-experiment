# WGPU + Tauri Experiment

This repo is an attempt at using the Rust [wgpu](https://github.com/gfx-rs/wgpu) crate to draw into (a portion of) a [Tauri](https://tauri.studio/) window.

As of right now, it does not work at all :D

I've only tried this on macOS, and it required hacking up wgpu to get it
to run without erroring. See [my fork of wgpu](https://github.com/dceddia/wgpu) where it grabs the `contentView` from the NSWindow.

It appears that it's able to create a surface and render to that
surface, but it doesn't show up visually. I'm not sure if that means
it's silently failing, or if the view it's drawing to is behind the
WebView, or if the WebView *replaced* the view that's being drawn to...
