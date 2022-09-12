# encode-microphone
encode-microphone demonstrates how to send audio to your browser from your microphone (input device).

## Instructions
### Build encode-microphone
```
cargo build --example encode-microphone
```

### Open encode-microphone example page
[jsfiddle.net](https://jsfiddle.net/9s10amwL/) you should see two text-areas and a 'Start Session' button

### Run encode-microphone with your browsers SessionDescription as stdin
In the jsfiddle the top textarea is your browser, copy that and:

#### Linux/macOS
Run `echo $BROWSER_SDP | ./target/debug/examples/encode-microphone`
 
#### Windows
1. Paste the SessionDescription into a file.
2. Run `./target/debug/examples/encode-microphone < my_file`

### Input encode-microphone's SessionDescription into your browser
Copy the text that `encode-microphone` just emitted and copy into second text area

### Hit 'Start Session' in jsfiddle, enjoy your video!
Audio should start playing in your browser below the input boxes.

Congrats, you have used WebRTC.rs!
