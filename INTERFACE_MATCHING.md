# Interface Matching Guide

bandwidthmon supports flexible interface matching to make it easier to specify network interfaces.

## How Interface Matching Works

### 1. Exact Match (Highest Priority)

If the pattern exactly matches an interface name, it will be selected:

```bash
# Windows
bandwidthmon -i "vEthernet (realtek)"

# Linux
bandwidthmon -i eth0
bandwidthmon -i wlan0

# macOS
bandwidthmon -i en0
```

### 2. Partial Match (Case-Insensitive)

If no exact match is found, bandwidthmon searches for interfaces containing the pattern (case-insensitive):

```bash
# These will match "vEthernet (realtek)":
bandwidthmon -i realtek    # ✓ matches
bandwidthmon -i Real       # ✓ matches
bandwidthmon -i REALTEK    # ✓ matches
bandwidthmon -i veth       # ✓ matches

# These will match "Ethernet (eth0)":
bandwidthmon -i eth        # ✓ matches
bandwidthmon -i Ether      # ✓ matches
```

### 3. Multiple Matches

If multiple interfaces match the pattern, bandwidthmon automatically selects the **shortest** interface name (most specific):

```bash
# If you have:
# - "Ethernet (eth0)-WFP Native MAC Layer LightWeight Filter-0000"
# - "Ethernet (eth0)"
# - "vEthernet (realtek)"

bandwidthmon -i eth
# Will select: "Ethernet (eth0)" (shortest match)

bandwidthmon -i realtek
# Will select: "vEthernet (realtek)" (only match)
```

## Platform-Specific Examples

### Windows

Windows interface names are often verbose:

```bash
# List interfaces first
bandwidthmon -l

# Output might show:
# - Ethernet (eth0)-WFP Native MAC Layer LightWeight Filter-0000
# - Ethernet (eth0)
# - vEthernet (realtek)
# - WiFi

# Quick matching:
bandwidthmon -i eth       # Matches "Ethernet (eth0)"
bandwidthmon -i realtek   # Matches "vEthernet (realtek)"
bandwidthmon -i wifi      # Matches "WiFi"
```

### Linux

Linux interfaces have simpler names:

```bash
# List interfaces
bandwidthmon -l

# Output might show:
# - eth0
# - wlan0
# - lo
# - docker0

# Matching (usually exact):
bandwidthmon -i eth0      # Exact match
bandwidthmon -i wlan      # Matches "wlan0"
bandwidthmon -i lo        # Exact match
```

### macOS

macOS uses en* naming:

```bash
# List interfaces
bandwidthmon -l

# Output might show:
# - en0 (WiFi)
# - en1 (Thunderbolt)
# - lo0

# Matching:
bandwidthmon -i en0       # Exact match
bandwidthmon -i wifi      # Matches "en0 (WiFi)"
bandwidthmon -i thunder   # Matches "en1 (Thunderbolt)"
```

## Error Messages

### No Match Found

```bash
bandwidthmon -i xyz

# Error output:
Error: No interface matches 'xyz'. Available interfaces:
Ethernet (eth0)
vEthernet (realtek)
WiFi
```

The error message shows all available interfaces to help you choose.

## Best Practices

### 1. List Interfaces First

Always start by listing available interfaces:

```bash
bandwidthmon -l
```

### 2. Use Unique Substrings

Choose the most unique part of the interface name:

```bash
# Good (unique):
bandwidthmon -i realtek

# Less ideal (might match multiple):
bandwidthmon -i eth
```

### 3. Use Quotes for Names with Spaces

```bash
# Windows
bandwidthmon -i "vEthernet (realtek)"
bandwidthmon -i "Ethernet (eth0)"

# Or use partial match without quotes:
bandwidthmon -i realtek
```

### 4. Case Doesn't Matter

```bash
# All equivalent:
bandwidthmon -i REALTEK
bandwidthmon -i realtek
bandwidthmon -i Realtek
```

## Troubleshooting

### Interface Not Found

**Problem:** Getting "Interface not found" error

**Solution:**
1. List all interfaces: `bandwidthmon -l`
2. Find the correct name or unique substring
3. Try partial match with a unique part

**Example:**
```bash
# List interfaces
bandwidthmon -l
# Shows: "vEthernet (realtek)"

# Use partial match
bandwidthmon -i realtek  # ✓ Works!
```

### Multiple Matches Warning

**Problem:** Pattern matches multiple interfaces

**Solution:**
The tool automatically selects the shortest name. To be more specific:
1. Use a more unique substring
2. Use the full interface name in quotes

**Example:**
```bash
# If "eth" matches both:
# - "Ethernet (eth0)"
# - "Ethernet (eth0)-WFP Native..."

# Option 1: More specific pattern
bandwidthmon -i "eth0)"

# Option 2: Full name
bandwidthmon -i "Ethernet (eth0)"
```

### Special Characters

**Problem:** Interface name has special characters

**Solution:** Use quotes:

```bash
# Correct:
bandwidthmon -i "vEthernet (realtek)"

# Wrong (shell interprets parentheses):
bandwidthmon -i vEthernet (realtek)
```

## Examples from Your Output

Based on your system's output:

```
Available interfaces:
- Ethernet (eth0)-WFP Native MAC Layer LightWeight Filter-0000
- Ethernet (eth0)
- vEthernet (realtek)
```

### To monitor realtek:

```bash
# Best (most specific):
bandwidthmon -i realtek

# Also works:
bandwidthmon -i "vEthernet (realtek)"
bandwidthmon -i veth
bandwidthmon -i REAL
```

### To monitor eth0:

```bash
# Best (avoids WFP filter):
bandwidthmon -i "Ethernet (eth0)"

# Also works (selects shortest):
bandwidthmon -i eth0

# Works but less specific:
bandwidthmon -i eth
```

## Advanced Usage

### Auto-Select Best Interface

Don't specify any interface to auto-select the one with most traffic:

```bash
bandwidthmon
```

This selects the interface with the highest total bytes (received + transmitted).

### Monitor Specific Interface Types

```bash
# WiFi interfaces
bandwidthmon -i wifi
bandwidthmon -i wlan

# Ethernet interfaces
bandwidthmon -i eth

# Virtual interfaces
bandwidthmon -i veth
bandwidthmon -i docker

# Loopback
bandwidthmon -i lo
```

## Summary

- **Exact match** is tried first
- **Partial match** is case-insensitive
- **Shortest name** wins for multiple matches
- **Use quotes** for names with spaces/special characters
- **List first** with `-l` to see available interfaces
- **Be specific** to avoid ambiguity

For the most reliable results, use the **exact interface name** in quotes!