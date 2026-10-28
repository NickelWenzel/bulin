# bulin

bulin is a live editor for fragment shaders: the user writes a shader, adjusts the values it
reads by dragging across them, and watches the result.

## Language

**Shader**:
The WESL module the user edits. It defines `mainBulin`, which bulin calls for every pixel, and
imports the uniforms it reads from the uniforms package.
_Avoid_: fragment shader, entry point

**Uniform**:
A named value the user adds and adjusts by dragging across its fields, such as a radius or a
color. A shader reads it as a member of the imported `customs` struct, as in `customs.radius`.
_Avoid_: custom, parameter

**Time**:
The built-in uniform: the seconds on the clock, read as `customs.time` while the clock is on.

**Layout**:
The shape of the uniforms a shader can read: whether time is on, and each uniform's name and type,
in order. Editing a value or the clock ticking changes values, not the layout.

**Uniforms package**:
The package bulin generates from the uniforms and time, named `uniforms`. It holds one struct
value, `customs`, that a shader imports with `import uniforms::customs;`.

**Pixel**:
What bulin tells `mainBulin` about the pixel it draws: its normalized coordinates `uv` and its
framebuffer `position`.

**bulin package**:
The fixed package bulin provides to every shader, named `bulin`. It holds `Pixel`.

**Program**:
The shader together with the uniforms package and the bulin package. The viewer compiles and runs
it.

**Project**:
The saved file holding a shader, its uniforms and the clock.

**Settings**:
The editor options and shortcuts bulin keeps for the user across projects, such as the font size
or the chord that toggles inlay hints.
_Avoid_: preferences, config
