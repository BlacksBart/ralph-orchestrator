"""Freeze terminal capture utilities for TUI validation."""

import asyncio
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Optional, Literal
from datetime import datetime


OutputFormat = Literal["svg", "png", "text"]


@dataclass
class CaptureResult:
    """Result of a freeze capture operation."""

    text_path: Path
    svg_path: Optional[Path]
    png_path: Optional[Path]
    raw_content: str


class FreezeCapture:
    """Captures terminal output to images using freeze CLI.

    Uses charmbracelet/freeze for high-fidelity terminal screenshots.
    """

    def __init__(self, output_dir: Optional[Path] = None):
        """Initialize freeze capture.

        Args:
            output_dir: Directory to save captures. Defaults to temp directory.
        """
        self.output_dir = output_dir or Path(tempfile.gettempdir())

    async def capture_buffer(
        self,
        content: str,
        name_prefix: str = "capture",
        formats: tuple[OutputFormat, ...] = ("svg", "png", "text"),
    ) -> CaptureResult:
        """Capture a text buffer to screenshot.

        Args:
            content: The terminal content (may include ANSI codes)
            name_prefix: Prefix for output filenames
            formats: Output formats to generate

        Returns:
            CaptureResult with paths to generated files
        """
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        base_name = f"{name_prefix}_{timestamp}"

        # Always save raw text
        text_path = self.output_dir / f"{base_name}.txt"
        text_path.write_text(content)

        svg_path = None
        png_path = None

        # Generate SVG if requested
        if "svg" in formats:
            svg_path = self.output_dir / f"{base_name}.svg"
            await self._run_freeze(text_path, svg_path, "svg")

        # Generate PNG if requested
        if "png" in formats:
            png_path = self.output_dir / f"{base_name}.png"
            await self._run_freeze(text_path, png_path, "png")

        return CaptureResult(
            text_path=text_path,
            svg_path=svg_path,
            png_path=png_path,
            raw_content=content,
        )

    async def capture_file(
        self,
        input_path: Path,
        name_prefix: str = "capture",
        formats: tuple[OutputFormat, ...] = ("svg", "png"),
    ) -> CaptureResult:
        """Capture an existing file to screenshot.

        Args:
            input_path: Path to file containing terminal output
            name_prefix: Prefix for output filenames
            formats: Output formats to generate

        Returns:
            CaptureResult with paths to generated files
        """
        content = input_path.read_text()
        return await self.capture_buffer(content, name_prefix, formats)

    async def _run_freeze(
        self,
        input_path: Path,
        output_path: Path,
        output_format: str,
    ) -> None:
        """Run freeze CLI to generate screenshot.

        Args:
            input_path: Path to input file
            output_path: Path for output file
            output_format: Output format (svg or png) - determined by output file extension
        """
        # Freeze determines format from output file extension
        cmd = [
            "freeze",
            str(input_path),
            "--language", "ansi",
            "--theme", "base16",
            "-o", str(output_path),
        ]

        proc = await asyncio.create_subprocess_exec(
            *cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, stderr = await proc.communicate()

        if proc.returncode != 0:
            # Don't fail the whole test if PNG generation fails
            # (may not have all dependencies for rasterization)
            import logging
            logging.warning(
                f"freeze {output_format} generation failed: {stderr.decode()}"
            )

    @staticmethod
    def is_available() -> bool:
        """Check if freeze CLI is available on the system."""
        try:
            result = subprocess.run(
                ["freeze", "--version"],
                capture_output=True,
                text=True,
            )
            return result.returncode == 0
        except FileNotFoundError:
            return False
