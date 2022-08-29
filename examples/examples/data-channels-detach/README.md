# data-channels
data-channels is a WebRTC.rs application that shows how you can send/recv DataChannel messages from a web browser

## Instructions
### Build data-channels-detach
```
cargo build --example data-channels-detach
```

### Open data-channels-detach example page
[jsfiddle.net](https://jsfiddle.net/9tsx15mg/90/)

### Run data-channels-detach, with your browsers SessionDescription as stdin
In the jsfiddle the top textarea is your browser's session description, copy that and:
#### Linux/macOS
Run `echo $BROWSER_SDP | ./target/debug/examples/data-channels-detach`
#### Windows
1. Paste the SessionDescription into a file.
1. Run `./target/debug/examples/data-channels-detach < my_file`

### Input data-channels-detach's SessionDescription into your browser
Copy the text that `data-channels` just emitted and copy into second text area

### Hit 'Start Session' in jsfiddle
Under Start Session you should see 'Checking' as it starts connecting. If everything worked you should see `New DataChannel foo 1`

Now you can put whatever you want in the `Message` textarea, and when you hit `Send Message` it should appear in your terminal!

WebRTC.rs will send random messages every 5 seconds that will appear in your browser.

Congrats, you have used WebRTC.rs!
