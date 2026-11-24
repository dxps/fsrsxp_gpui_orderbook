#!/bin/sh

cargo watch -w crates/feed -w crates/proto -x 'run -p fsrsxp-orderbook-feed'

