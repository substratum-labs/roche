# Substratum Roche Space Ops Sandbox Image
# Image Tag: substratum/roche-space-ops:v0.1.0
FROM python:3.11-slim-bookworm

LABEL maintainer="Substratum Labs <dev@substratum.io>"
LABEL roche.environment="argonavis-triage-env"
LABEL roche.version="v0.1.0"
LABEL roche.capability.space_ops="true"

ENV DEBIAN_FRONTEND=noninteractive \
    PYTHONUNBUFFERED=1 \
    PYTHONDONTWRITEBYTECODE=1 \
    OREKIT_DATA_PATH=/opt/space-data/orekit-data \
    ASTROPY_CACHE_DIR=/opt/space-data/astropy-cache \
    ASTROPY_OFFLINE=1 \
    OFFLINE_MODE=1

RUN apt-get update && apt-get install -y --no-install-recommends \
    openjdk-17-jre-headless \
    curl \
    unzip \
    ca-certificates \
    libgomp1 \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd -g 1000 spaceops \
    && useradd -u 1000 -g spaceops -m -s /bin/bash spaceops \
    && mkdir -p /opt/space-data/orekit-data \
        /opt/space-data/astropy-cache \
        /opt/space-data/swpc \
        /tmp/workspace \
    && chown -R spaceops:spaceops /opt/space-data /tmp/workspace

RUN pip install --no-cache-dir \
    numpy==1.26.4 \
    scipy==1.12.0 \
    pandas==2.2.1 \
    matplotlib==3.8.3 \
    pydantic==2.6.4 \
    astropy==6.0.0 \
    sgp4==2.23 \
    orekit-jpype==12.1.2.0 \
    ccsds-ndm-py==0.0.3

WORKDIR /opt/space-data/orekit-data
RUN curl -fsSL https://gitlab.orekit.org/orekit/orekit-data/-/archive/main/orekit-data-main.zip \
        -o orekit-data.zip \
    && unzip -q orekit-data.zip \
    && mv orekit-data-main/* . \
    && rm -rf orekit-data-main orekit-data.zip \
    && chown -R spaceops:spaceops /opt/space-data/orekit-data

USER spaceops
WORKDIR /tmp/workspace

RUN python3 -c "import astropy, ccsds_ndm, sgp4; print('Astrodynamics modules loaded successfully.')" \
    && python3 -c "import orekit_jpype; orekit_jpype.initVM(); print('Orekit JVM initialized successfully.')"

CMD ["python3"]
