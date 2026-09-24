# Raspberry application for buzzers

To build on WSL2:

install rust in wsl2.

( rustup target add aarch64-unknown-linux-gnu) may not be neccessary

cargo install cross

You also need Docker running on your computer. If needed install Docker desktop and start it *in windows(

In WSL:
cross build --target aarch64-unknown-linux-gnu --release

To copy the binary to Embeddy raspberry pi:
scp target/aarch64-unknown-linux-gnu/release/my_pi_project embeddy@192.168.4.2:~
Password: Gbg1

You can move other files you need on the Rapberry in the same way with SCP.

# Physical connections:

## Wire from buzzer buttons:
Brown   Led GND
White   Led + (3.3V) GPIO Output
Yellow  button a
Green   button b (a annd b connected when button pressed)

# buzzer connection to Raspberry pi GPIO:

all Brown and Yellow wires to GND

Yellow Button       GPIO17  Led + (white wire)
                    GPIO27  Button b (green wire)

Red Button          GPIO22  Led + (white wire)
                    GPIO10  Button b (green wire)

Blue Button         GPIO9   Led + (white wire)
                    GPIO11  Button b (green wire)

Green Button        GPIO5   Led + (white wire)
                    GPIO6   Button b (green wire)

White Button        GPIO13  Led + (white wire)
                    GPIO19  Button b (green wire)

# Rest API

The buzzers can be read using a get operation
http://localhost:3030/getPressedButton

returns a json

To reset Buzzers again Post
http://localhost:3030/reset


