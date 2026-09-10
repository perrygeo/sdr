import os
import re

import matplotlib.pyplot as plt
import numpy as np
from rtlsdr import RtlSdr

ranges_continuous = {
    (87, 108): "FM Radio",
    (108, 118): "VOR/ILS Aviation",
    (118, 137): "Civil Air-band",
    (162, 163): "NOAA Weather Radio",
    (470, 608): "UHF TV (ATSC)",
    (617, 652): "Cellular 600 MHz",
    (728, 756): "Cellular 700 MHz",
    (869, 894): "Cellular 850 MHz",
    (1090, 1091): "ADS-B 1090 MHz",
}

ranges_maybe = {
    (144, 148): "2m HAM",
    (152, 153): "Paging 152 MHz",
    (156, 162): "VHF Marine + AIS",
    (220, 222): "Positive Train Control",
    (400, 406): "Radiosonde 400 MHz",
    (420, 450): "70cm HAM",
    (454, 460): "Paging 454 MHz",
    (462, 467): "GMRS/FRS",
    (902, 928): "ISM / 33cm HAM",
    (929, 932): "Paging 929 MHz",
    (960, 1089): "DME/TACAN 960-1089",
    (1091, 1215): "DME/TACAN 1091-1215",
    (1240, 1300): "23cm HAM",
    (1616, 1627): "Iridium Downlink",
    (1675, 1683): "Radiosonde 1675 MHz",
    (1710, 1755): "Cellular AWS Uplink",
}

ranges_not_likely = {
    (54, 88): "VHF-low TV (ch 2-6)",
    (137, 138): "NOAA APT / Orbcomm",
    (138, 144): "Federal Land Mobile 138",
    (148, 151): "Federal Land Mobile 148",
    (216, 220): "AMTS / Fixed",
    (225, 400): "Military UHF",
    (406, 420): "Federal Land Mobile / SARSAT",
    (608, 614): "Channel 37",
    (1400, 1427): "Radio Astronomy 1400",
    (1525, 1559): "Inmarsat L-band",
    (1559, 1610): "GNSS (GPS L1 etc.)",
    (1670, 1675): "Radio Astronomy 1670",
    (1683, 1710): "GOES / MetOp",
}

step = 2
fft_size = 512
num_rows = 500
sample_rate = 2.4e6  # Hz
freq_correction = 60  # PPM
gain = 49.6


def likely_centers_mhz():
    blocks = {}
    for k, v in ranges_continuous.items():
        centers = list(range(k[0], k[1], step))
        blocks[v] = centers
    for k, v in ranges_maybe.items():
        centers = list(range(k[0], k[1], step))
        blocks[v] = centers

    return blocks


def waterfall_for(center_freq, outdir="/tmp/waterfall", title="waterfall"):

    os.makedirs(outdir, exist_ok=True)

    if not (24e6 < center_freq < 1766e6):
        msg = f"{center_freq / 1e6:.1f} MHz out of RTL-SDR range (24–1766 MHz)"
        print(msg, end="...", flush=True)
        return

    sdr.center_freq = center_freq

    # get rid of any initial empty samples ??
    x = sdr.read_samples(2048)

    # get all the samples we need for the spectrogram
    x = sdr.read_samples(fft_size * num_rows)

    spectrogram = np.zeros((num_rows, fft_size))
    for i in range(num_rows):
        spectrogram[i, :] = 10 * np.log10(
            np.abs(np.fft.fftshift(np.fft.fft(x[i * fft_size : (i + 1) * fft_size])))
            ** 2
        )
    extent = (
        (center_freq + sdr.sample_rate / -2) / 1e6,
        (center_freq + sdr.sample_rate / 2) / 1e6,
        len(x) / sdr.sample_rate,
        0,
    )

    # print(spectrogram.min(), spectrogram.max())
    plt.figure(figsize=[2 * s for s in plt.rcParams["figure.figsize"]])
    plt.imshow(
        spectrogram, aspect="auto", extent=extent, cmap="cividis", vmin=-50, vmax=50
    )
    plt.xlabel("Frequency [MHz]")
    plt.ylabel("Time [s]")
    plt.title(title)
    nice_title = re.sub(r'[<>:"/\\|?*\x00-\x1f]', "_", title).strip()
    center_freq_mhz = int(center_freq / 1e6)
    path = f"{outdir}/{center_freq_mhz}MHz-{nice_title}.png"
    plt.savefig(path)
    plt.close()

    return path


if __name__ == "__main__":
    sdr = RtlSdr()

    sdr.sample_rate = sample_rate  # Hz
    sdr.freq_correction = freq_correction  # PPM

    assert gain in sdr.valid_gains_db
    sdr.gain = gain

    frequencies = likely_centers_mhz()

    for block, centers in frequencies.items():
        for center_mhz in centers:
            print(center_mhz, "MHz...", end=" ", flush=True)
            print(waterfall_for(center_mhz * 1e6, title=block))

    sdr.close()
