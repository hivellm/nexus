"""Setup script for Nexus project.

This is a Rust project with Python scripts. This setup.py exists only to prevent
setuptools from auto-discovering packages when pip tries to install dependencies.
"""

from setuptools import setup

setup(
    name="nexus",
    version="0.0.0",
    description="Nexus Graph Database - Rust project with Python scripts",
    packages=[],  # Explicitly empty - this is not a Python package
    install_requires=[],  # Dependencies are managed elsewhere
)

