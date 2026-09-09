import os

import matplotlib.pyplot as plt
import numpy as np
from rtlsdr import RtlSdr

fft_size = 512
num_rows = 500
frequencies = range(85, 170, 2)

sample_rate = 2.4e6  # Hz
freq_correction = 60  # PPM
gain = 49.6


def waterfall_for(center_freq, outdir="/tmp/waterfall"):

    os.makedirs(outdir, exist_ok=True)

    assert 24e6 < center_freq < 1766e6, (
        f"center_freq {center_freq / 1e6:.1f} MHz out of RTL-SDR range (24–1766 MHz)"
    )
    # mutate
    sdr.center_freq = center_freq

    # get rid of initial empty samples
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
    center_freq_mhz = int(center_freq / 1e6)
    path = f"{outdir}/{center_freq_mhz}MHz.png"
    plt.savefig(path)
    plt.close()

    return path


if __name__ == "__main__":
    sdr = RtlSdr()

    sdr.sample_rate = sample_rate  # Hz
    sdr.freq_correction = freq_correction  # PPM

    assert gain in sdr.valid_gains_db
    sdr.gain = gain

    for center_mhz in frequencies:
        print(center_mhz, "MHz...", end=" ", flush=True)
        print(waterfall_for(center_mhz * 1e6))

    sdr.close()
