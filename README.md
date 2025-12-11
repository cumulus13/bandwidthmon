# bandwidthmon

[![Crates.io](https://img.shields.io/crates/v/bandwidthmon.svg)](https://crates.io/crates/bandwidthmon)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)

Real-time network bandwidth monitor with beautiful ASCII charts for the terminal.

<p align="center">
  <img src="https://raw.githubusercontent.com/cumulus13/bandwidthmon/master/bandwidthmon_1.png" alt="Bandwidthmon Line">
</p>

<p align="center">
  <img src="https://raw.githubusercontent.com/cumulus13/bandwidthmon/master/bandwidthmon_2.png" alt="Bandwidthmon Graph">
</p>

<p align="center">
  <img src="https://raw.githubusercontent.com/cumulus13/bandwidthmon/master/bandwidthmon_3.png" alt="Bandwidthmon rasciichart">
</p>

## Features

- 📊 **Beautiful ASCII Charts** - Smooth line rendering with box-drawing characters
- 🎯 **Two Versions** - Choose between `rasciichart` library or manual rendering
- ⚡ **Real-time Monitoring** - Live bandwidth statistics with 1-second updates
- 🔍 **Smart Interface Matching** - Partial and case-insensitive interface names
- 📈 **Statistics** - Track peak, average, and total bandwidth usage
- 🎨 **Colorful Output** - Color-coded download/upload charts
- ⌨️ **Interactive** - Keyboard controls for easy navigation
- 🔍 **Flexible Filtering** - Show download only, upload only, or both
- 📱 **Auto-sizing** - Charts automatically fit your terminal width
- 🌐 **Cross-platform** - Works on Windows, Linux, macOS, and BSD

## Installation

```bash
cargo install bandwidthmon
```

Or build from source:

```bash
git clone https://github.com/cumulus13/bandwidthmon
cd bandwidthmon
cargo build --release
```

## Usage

### bandwidthmon (using rasciichart)

```bash
# Auto-select best interface
bandwidthmon

# Monitor specific interface (supports partial matching!)
bandwidthmon -i eth0
bandwidthmon -i realtek    # Matches "vEthernet (realtek)" on Windows
bandwidthmon -i wlan       # Matches "wlan0" on Linux

# Custom chart size
bandwidthmon -H 15 -W 100

# Show summary statistics
bandwidthmon -s

# Show download only
bandwidthmon -d

# Show upload only
bandwidthmon -u

# List available interfaces
bandwidthmon -l
```

### bandwidthmon2 (manual rendering)

Same arguments as `bandwidthmon`, but uses manual graph rendering:

```bash
bandwidthmon2 -i wlan0 -H 20 -s
```

### bandwidthmon3 (other version rasciichart)

Same arguments as `bandwidthmon`, but uses manual less color:

```bash
bandwidthmon3 -i wlan0 -H 20 -s
```

## Command-line Options

```
Options:
  -i, --iface <IFACE>      Network interface to monitor (auto-select if not specified)
  -H, --height <HEIGHT>    Chart height in lines [default: 10]
  -W, --width <WIDTH>      Chart width in columns (auto-fit terminal if 0) [default: 0]
  -l, --list               List available network interfaces
  -s, --summary            Show summary statistics
  -d, --download           Show download chart only
  -u, --upload             Show upload chart only
      --history <HISTORY>  Maximum history points [default: 120]
  -h, --help               Print help
  -V, --version            Print version
```

## Keyboard Controls

- `q` or `Q` - Quit
- `Esc` - Quit
- `Ctrl+C` - Quit

## Summary Statistics

Use `-s` or `--summary` to show additional statistics:

- **Peak DL/UL** - Maximum download/upload speeds
- **Avg DL/UL** - Average download/upload speeds
- **Total RX/TX** - Total bytes received/transmitted
- **Runtime** - Monitoring session duration

## Dependencies

- `sysinfo` - System and network information
- `crossterm` - Terminal manipulation
- `rasciichart` - ASCII chart rendering (bandwidthmon only)
- `clap` - Command-line argument parsing
- `anyhow` - Error handling

## Platform Support

- ✅ Linux
- ✅ macOS
- ✅ Windows
- ✅ BSD

## Performance

- Minimal CPU usage (~0.5%)
- Low memory footprint (~5 MB)
- Configurable history size for memory optimization

## License

MIT License - see [LICENSE](LICENSE) file for details

## Author

**Hadi Cahyadi**
- Email: cumulus13@gmail.com
- GitHub: [cumulus13](https://github.com/cumulus13)

[![Buy Me a Coffee](https://www.buymeacoffee.com/assets/img/custom_images/orange_img.png)](https://www.buymeacoffee.com/cumulus13)

[![Donate via Ko-fi](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/cumulus13)
 
[Support me on Patreon](https://www.patreon.com/cumulus13)

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## Related Projects

- [rasciichart](https://crates.io/crates/rasciichart) - ASCII chart library for Rust

## FAQ

**Q: How do I monitor multiple interfaces?**  
A: Run multiple instances in different terminals with `-i` for each interface.

**Q: Can I export the data?**  
A: Currently, the tool is for real-time monitoring only. Export functionality may be added in future versions.

**Q: The chart looks weird on my terminal**  
A: Ensure your terminal supports UTF-8 and box-drawing characters. Try a modern terminal like Alacritty, iTerm2, or Windows Terminal.

## Support

If you encounter any issues or have suggestions, please open an issue on [GitHub](https://github.com/cumulus13/bandwidthmon/issues).