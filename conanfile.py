import os

from conan import ConanFile
from conan.tools.files import copy


class Recipe(ConanFile):
    settings = "os", "compiler", "build_type", "arch"
    generators = "CMakeToolchain", "CMakeDeps", "VirtualRunEnv"

    def layout(self):
        self.folders.generators = "build/conan"

    def requirements(self):
        self.requires("cereal/1.3.2")
        self.requires("sdl/2.30.7")
        self.requires("immer/0.8.1")
        self.requires("lager/0.1.1")
        self.requires("imgui/1.91.0-docking")

    def build_requirements(self):
        self.test_requires("catch2/3.7.0")

    def configure(self):
        self.options["sdl"].pulse = False # Avoid problems with clang-18

    def generate(self):
        copy(self, "*sdl*", os.path.join(self.dependencies["imgui"].package_folder,
            "res", "bindings"), os.path.join(self.source_folder, "bindings"))
        copy(self, "*opengl3*", os.path.join(self.dependencies["imgui"].package_folder,
            "res", "bindings"), os.path.join(self.source_folder, "bindings"))