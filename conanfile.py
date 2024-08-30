from conan import ConanFile


class Recipe(ConanFile):
    settings = "os", "compiler", "build_type", "arch"
    generators = "CMakeToolchain", "CMakeDeps", "VirtualRunEnv"

    def layout(self):
        self.folders.generators = f"build/conan/{self.settings.build_type}"

    def requirements(self):
        self.requires("fmt/11.0.2")
        self.requires("lager/0.1.1")

    def build_requirements(self):
        self.test_requires("catch2/3.7.0")
