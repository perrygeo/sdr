SHELL := bash

_default:
	@echo "For ADS-B aircraft signals - make dump1090|tar1090|view|aircraft"
	@echo "For spectrum analysis      - make scan"

# dump1090-fa (FlightAware fork) + tar1090 (docker) wiring.
#
# Prerequisite (one-time if using NixOS): the firewall drops bridge->host INPUT on
# 30005, so the container's readsb times out reaching host.docker.internal.
#   networking.firewall.interfaces.docker0.allowedTCPPorts = [ 30005 ];
#   sudo nixos-rebuild switch

BEAST_PORT        ?= 30005
DUMP1090          ?= dump1090
DUMP1090_OPTS     ?=
LAT               ?= 40.5
LONG              ?= -105.0
RTL_DEVICE        ?= 0
TAR1090_HTTP_PORT ?= 8078
SAMPLE_SECONDS    ?= 60
TAR1090_IMAGE     ?= ghcr.io/sdr-enthusiasts/docker-tar1090:latest
TZ                ?= UTC

.PHONY: dump1090 tar1090 _default scan view aircraft


dump1090: ## run dump1090, emitting Beast on TCP $(BEAST_PORT)
	$(DUMP1090) \
		--device $(RTL_DEVICE) \
		--net \
		--quiet \
		--ppm 60 \
		--gain 49.6 \
		--stats \
		--interactive \
		--net-bo-port $(BEAST_PORT) \
		$(DUMP1090_OPTS)

tar1090: ## run tar1090 (docker), consuming Beast from the host's dump1090
	@test -n "$(LAT)" -a -n "$(LONG)" || { echo "error: LAT and LONG are required, e.g. make tar1090 LAT=40.71 LONG=-74.01"; exit 1; }
	docker run --rm --name tar1090 \
		-p $(TAR1090_HTTP_PORT):80 \
		--add-host host.docker.internal:host-gateway \
		-e TZ=$(TZ) \
		-e BEASTHOST=host.docker.internal \
		-e BEASTPORT=$(BEAST_PORT) \
		-e LAT=$(LAT) \
		-e LONG=$(LONG) \
		--tmpfs=/run:exec,size=256M \
		$(TAR1090_IMAGE)

view:
	chromium --app="http://localhost:8078" --class=tar1090 --allow-insecure-localhost

aircraft:
	@echo SAMPLE_SECONDS=$(SAMPLE_SECONDS)
	T=$$(date +%s); \
	uv run src/sdr/read_dump1090.py -s $(SAMPLE_SECONDS) \
     | gdal vector convert \
        --if GeoJSONSeq '/vsistdin?buffer_limit=250000000' \
        --output-layer airplanes \
        outputs/aircraft-$$T.geojson && echo outputs/aircraft-$$T.geojson

scan:
	uv run src/sdr/scan_waterfall.py
