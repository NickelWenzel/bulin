# bulin

bulin is a live editor for fragment shaders: the user writes a shader, adjusts the values it
reads by dragging across them, and watches the result.

## Language

**Shader**:
The WESL code the preview runs: a project's text, or a folder's main file and what it imports. It
defines `mainBulin`, which bulin calls for every pixel, and imports the uniforms it reads from the
uniforms package.
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
The main file with the modules it imports, the uniforms package and the bulin package. The viewer
compiles and runs it.

**Project**:
The single `.bulin` file holding a shader's text, its uniforms and the clock.

**Settings**:
The editor options and shortcuts bulin keeps for the user across projects, such as the font size
or the chord that toggles inlay hints.
_Avoid_: preferences, config

**Folder**:
A directory the user opens in bulin. Its shader files form one WESL package.

**Main file**:
The folder file the preview runs. It defines `mainBulin` and may import the folder's other files.
A project's text is its main file.
_Avoid_: entry (a WGSL entry point is something else)

**Document**:
The one file open in the editor. In a folder it need not be the main file.

**Folder settings**:
A folder's bulin state, kept in `.bulin/settings.json`: its main file, uniforms and clock. Unlike
the user's settings, they belong to the folder and travel with it.

**Library**:
On the web, the folders and projects the browser stores for bulin.
