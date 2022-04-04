BINS = \
	block1

RUSTC = rustc

all: $(BINS)

%: %.rs
	$(RUSTC) -o $* $<
