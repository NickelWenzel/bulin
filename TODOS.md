# Project
- ❌ Re-integrate rust branch on master
- ❌ Adjust CI-pipeline to rust native and wasm builds
- ❌ Add Readme 
- ❌ VSCode debug configs for wasm

# Iced text editor enhancements
>All of this should be implemented in the iced fork an potentially be upstreamed
- ❌ Accept custom highlighter creator
- ❌ Show line numbers: research if this can be added to existing `TextEditor`
- ❌ Undo/redo: This already exists in `cosmic::TextEdit` and hopefully just have to be connected

# Bulin text editor enhancements
- ❌ Icon row with word wrap etc. maybe introduce modal settings menu for wordwrap and theme selection
- ❌ Refactor serialization: Wrap `text_editor::Contents` in newtype and implement serialization in terms of string serialization just for this and just derive the rest of `TextEditor` serialization
- ❌ Uniforms to shader variables functionality

# Uniforms enhancements
- ❌ Add colorpicker
- ❌ Add vector inputs maybe some via length and draggable vec
- ❌ Add matrix inputs via translation, scale, sheer and draggable coordinate system for rotation

# Include shader code functions
- ❌ Add additional shader text editor tabs
- ❌ List additonal tabs
- ❌ Reference and include tabs in main shader
- ❌ Load shader code frokm web via some API
- ❌ Add preview for defined function interfaces via some preview window

# Scene builder
- ❌ Add different Builder project type
- ❌ Define some basic building blocks
- ❌ Add some UI that can plug blocks together without needing code
- ❌ Convert builder project to plain code
- ❌ Add preview for blocks

# Application
- ❌ Export to shadertoy etc.
