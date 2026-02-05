use notify_rust::{Hint, Notification, Timeout};

fn main() {
    println!("Creating notification...");
    
    let notification = Notification::new()
        .summary("Meeting Detected - Click to Record")
        .body("Test meeting\n\nClick this notification to start recording, or dismiss to skip.")
        .icon("video-display")
        .action("default", "Start Recording")
        .hint(Hint::Transient(true))
        .timeout(Timeout::Milliseconds(10000))
        .show();

    match notification {
        Ok(handle) => {
            println!("Notification shown, waiting for action...");
            handle.wait_for_action(|action| {
                println!("Action received: {}", action);
            });
            println!("Done waiting");
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }
}
