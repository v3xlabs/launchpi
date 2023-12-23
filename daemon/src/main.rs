use launchy::launchpad_mini as mini;
use launchy::InputDevice;
use launchy::MsgPollingWrapper;
use launchy::OutputDevice;

fn main() {
    println!("Launching");

    let input = mini::Input::guess_polling().unwrap();
    let mut output = mini::Output::guess().unwrap();

    println!("Device found");

    for msg in input.iter() {
        println!("Message: {:?}", msg);

        if let mini::Message::Press { button, .. } = msg {
            println!("Button pressed: {:?}", button);
            output
                .set_button(
                    button,
                    mini::Color::RED,
                    mini::DoubleBufferingBehavior::None,
                )
                .unwrap();
        } else if let mini::Message::Release { button, .. } = msg {
            println!("Button released: {:?}", button);
            output
                .set_button(
                    button,
                    mini::Color::GREEN,
                    mini::DoubleBufferingBehavior::None,
                )
                .unwrap();
        }
    }
}
