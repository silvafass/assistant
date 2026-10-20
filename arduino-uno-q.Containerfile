FROM --platform=linux/arm64 debian:trixie

RUN apt-get update && apt-get install --assume-yes \
    # Audio utilities
    alsa-utils pipewire-bin pulseaudio-utils \
    # ALSA development files
    libasound2-dev \
    # JACK
    libjack-jackd2-dev \
    # PipeWire
    libpipewire-0.3-dev \
    # PulseAudio
    libpulse-dev \
    # General dependencies
    libssl-dev ca-certificates
