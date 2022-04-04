BINS = block1 \
	   block2
	

RUSTC = rustc
RUSTFLAGS = --edition=2021

all: $(BINS)

clean:
	rm -f $(BINS)

%: %.rs
	$(RUSTC) $(RUSTFLAGS) -o $* $<
