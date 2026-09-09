# RTL-SDR on NixOS

Some notes on how I got my NixOS desktop wired up to RTL-SDR support.

It was annoying to get right, but like most things NixOS, it's now *done*. The configuration is forever in git.


## Hardware

RTL-SDR USB dongle (RTL2832U demodulator chip)

## Kernel drivers


```nix
hardware.rtl-sdr.enable = true;
```

This handles:

1. Blacklisting the in-kernel DVB-T drivers so they don't claim the dongle
2. Install the package's udev rules
3. Add the package to `environment.systemPackages`.
4. Ensure the `plugdev` group exists (`users.groups.plugdev = {}`).


## User access
Must be part of the `plugdev` group

```nix
extraGroups = [ "networkmanager" "wheel" "docker" "plugdev" ];
```

Lets SDR apps talk to the dongle without root.


## Library selection: 


nixpkgs exposes three variants of the library.
`rtl-sdr`, `rtl-sd-osmocon`, and `rtl-sdr-librtlsdr`

We want the later for the dithering API. In `configuration.nix:98`:

```nix
rtl-sdr-librtlsdr # Provides librtlsdr.so (plain rtl-sdr lacks dithering API)
```


## Linking trick

The shared library needs to be on `LD_LIBRARY_PATH` for some ctypes APIs to work. Classic NixOS issue. To hell with purity...

`configuration.nix:105-106`:

```nix
environment.variables.LD_LIBRARY_PATH = "${pkgs.rtl-sdr-librtlsdr}/lib";
```

This is **not** the nix-ld mechanism. It's probably possible through `nix-ld` but I didn't try


## Building against librtlsdr


Without the LD_LIBRARY_PATH trick,
for anything using pkg-config to probe system libs, such as a Rust build.


```sh
nix-shell -p rtl-sdr-librtlsdr pkg-config --run 'cargo install ...'
```


## SDR software stack


```nix
gqrx
sdrpp
cubicsdr
dump1090-fa
sdrangel
gnuradio
gnss-sdr
```

Each nixpkgs-packaged app links its own librtlsdr variant at build time, so none
of them depend on the `LD_LIBRARY_PATH` above. Verified backends:

- `gqrx` → `rtl-sdr` (blog fork)
- `sdrangel` → `rtl-sdr` (blog fork)
- `dump1090-fa` → `rtl-sdr` (blog fork)
- `sdrpp` → `rtl-sdr-osmocom` (nixpkgs note: "osmocom better w/ rtlsdr v4")
- `cubicsdr` → `soapysdr-with-plugins` (SoapySDR + its RTL-SDR plugin)
- `gnuradio` / `gnss-sdr` → via `gr-osmosdr`, whose RTL backends are `rtl = [ rtl-sdr ]` and `soapy = [ soapysdr-with-plugins ]`

So in practice this system has three different librtlsdr builds present: blog
(`rtl-sdr`), osmocom (`rtl-sdr-osmocom` via sdrpp), and the maintained fork
(`rtl-sdr-librtlsdr` on `LD_LIBRARY_PATH`) for python.

## Firewall

```nix
networking.firewall.allowedTCPPorts = [ 22 30005 ];
```

Allow port `30005`the standard Beast-format binary output port of
`dump1090-fa` (ADS-B/Mode-S decoder), used by ADS-B feeders.


## Python notes

Use uv for everything.

see `pyproject.toml` for dependencies

The `pyrtlsdr` module is worth noting since it needs the rtl-sdr shared library at runtime, via the LD_LIBRARY_PATH hack.
