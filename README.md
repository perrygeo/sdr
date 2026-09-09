# SDR Utils

A Makefile and a few python scripts to

Decode and visualize aircraft signals (ADS-B), using `dump1090` and `tar1090` projects

<img src="img/tar1090.webp">

Build `geojson` points from live aircraft messages
Visualizing here in qgis.

<img src="img/qgis.webp">

Analyze spectrograms, i.e. waterfall plots. Using `pysdr`

<img src="img/103MHz.webp">

See `make`

    $ make
    For ADS-B aircraft signals - make dump1090|tar1090|view|aircraft
    For spectrum analysis      - make scan
