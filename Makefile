ABI_DST_DIR ?= ./contracts/abi
ABI_SRC_SUBDIR := mvconfig mvsystem bksystem token authservice dex giver accumulator exchange

.PHONY: copy-abi
copy-abi:
ifeq ($(strip $(ABI_SRC_DIR)),)
	$(error You must provide ABI_SRC_DIR folder, e.g., make copy ABI_SRC_DIR=/path/to/source)
endif
	@echo "Clear $(ABI_DST_DIR)..."
	rm -rf $$ABI_DST_DIR

	@echo "Copying *.abi.json files from selected directories ($(ABI_SRC_SUBDIR)) in $(ABI_SRC_DIR) to $(ABI_DST_DIR)..."
	@for subdir in $(ABI_SRC_SUBDIR); do \
		if [ -d "$(ABI_SRC_DIR)/$$subdir" ]; then \
			mkdir -p $(ABI_DST_DIR)/$$subdir; \
			cp $(ABI_SRC_DIR)/$$subdir/*.abi.json $(ABI_DST_DIR)/$$subdir/ 2>/dev/null || true; \
			echo "Copied from $$subdir"; \
		else \
			echo "Warning: $(ABI_SRC_DIR)/$$subdir does not exist"; \
		fi \
	done
	@echo "Done."
