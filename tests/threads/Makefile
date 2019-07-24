CFLAGS ?= -Wall -Wextra -pedantic -fopenmp

all: threads
threads: threads.c
	$(CC) $(CFLAGS) threads.c -o threads
clean:
	rm -f threads
.PHONY: clean
