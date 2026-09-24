#bin/bash/
cross build --target aarch64-unknown-linux-gnu --release
scp target/aarch64-unknown-linux-gnu/release/game_show magkar@192.168.3.5:~/Knowit/