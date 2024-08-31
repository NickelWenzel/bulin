from conan import ConanFile


class Recipe(ConanFile):
    settings = "os", "compiler", "build_type", "arch"
    generators = "CMakeToolchain", "CMakeDeps", "VirtualRunEnv"

    def layout(self):
        self.folders.generators = "build/conan"

    def requirements(self):
        self.requires("fmt/11.0.2")
        self.requires("lager/0.1.1")
        self.requires("imgui/1.91.0-docking")

    def build_requirements(self):
        self.test_requires("catch2/3.7.0")
