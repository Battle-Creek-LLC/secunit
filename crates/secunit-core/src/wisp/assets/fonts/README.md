# Bundled fonts for WISP PDF export

These font files are embedded into the `secunit` binary (`include_bytes!`) and
loaded into the Typst renderer so WISP PDFs render identically on every machine
and in CI, without depending on system-installed fonts.

**Required files (vendor these — they are binary, not committed by the scaffold tool):**

- `Inter-Regular.ttf`, `Inter-Medium.ttf`, `Inter-SemiBold.ttf`, `Inter-Bold.ttf`
- `JetBrainsMono-Regular.ttf`, `JetBrainsMono-Medium.ttf`
- `OFL.txt` — the SIL Open Font License text covering both families.

Sources:
- Inter — https://github.com/rsms/inter (SIL OFL 1.1)
- JetBrains Mono — https://github.com/JetBrains/JetBrainsMono (SIL OFL 1.1)

Both are OFL-licensed, so redistribution inside the binary is permitted provided
`OFL.txt` ships alongside. Keep the file names above stable — the loader
references them by name.
