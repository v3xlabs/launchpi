use std::thread::sleep;

use launchy::launchpad_mini_mk3 as mini;
use launchy::launchpad_mini_mk3::PaletteColor;
use launchy::InputDevice;
use launchy::MsgPollingWrapper;
use launchy::OutputDevice;

fn main() {
    println!("Launching");

    let input = mini::Input::guess_polling().unwrap();
    let mut output = mini::Output::guess().unwrap();

    println!("Device found");

    output.clear().unwrap();

    sleep(std::time::Duration::from_millis(100));

    // BECAUSE CYAN MUST BE A COLOR
    output.send(&[0xB0, 0x63, PaletteColor::CYAN.id()]).unwrap();

    let start_time = std::time::Instant::now();
    let cooldown = 100; // 1 second

    for msg in input.iter() {
        if start_time.elapsed().as_millis() < cooldown {
            println!("Discarding message due to cooldown");
            continue;
        }

        println!("Message: {:?}", msg);

        if let mini::Message::Press { button, .. } = msg {
            println!("Button pressed: {:?}", button);

            let colors = [
                mini::PaletteColor::RED,
                mini::PaletteColor::GREEN,
                mini::PaletteColor::YELLOW,
                mini::PaletteColor::BLUE,
                mini::PaletteColor::WHITE,
                mini::PaletteColor::CYAN,
                mini::PaletteColor::SLIGHTLY_LIGHT_GREEN,
                mini::PaletteColor::DARK_GRAY,
            ];
            let random_color = colors[button.abs_x() as usize % colors.len()];

            output
                .set_button(button, random_color, mini::LightMode::Plain)
                .unwrap();

            match button {
                mini::Button::ControlButton { index: 5 } => {
                    output.clear().unwrap();
                },
                mini::Button::ControlButton { index: 7 } => {
                    output.send_sleep(mini::SleepMode::Sleep).unwrap();
    
                    sleep(std::time::Duration::from_millis(1000));
                    output.send_sleep(mini::SleepMode::Wake).unwrap();
                },
                _ => {},
            }
        } else if let mini::Message::Release { button, .. } = msg {
            println!("Button released: {:?}", button);
            // output
            //     .set_button(button, mini::PaletteColor::GREEN, mini::LightMode::Pulse)
            //     .unwrap();
        }
    }
}

// use std::error::Error;
// use std::io::{stdin, stdout, Write};

// use midir::{Ignore, MidiInput, MidiOutput};

// fn main() {
//     match run() {
//         Ok(_) => (),
//         Err(err) => println!("Error: {}", err),
//     }
// }

// fn run() -> Result<(), Box<dyn Error>> {
//     let mut midi_in = MidiInput::new("midir test input")?;
//     midi_in.ignore(Ignore::None);
//     let midi_out = MidiOutput::new("midir test output")?;

//     let mut input = String::new();

//     loop {
//         println!("Available input ports:");
//         for (i, p) in midi_in.ports().iter().enumerate() {
//             println!("{}: {}", i, midi_in.port_name(p)?);
//         }

//         println!("\nAvailable output ports:");
//         for (i, p) in midi_out.ports().iter().enumerate() {
//             println!("{}: {}", i, midi_out.port_name(p)?);
//         }

//         // run in endless loop if "--loop" parameter is specified
//         match ::std::env::args().nth(1) {
//             Some(ref arg) if arg == "--loop" => {}
//             _ => break,
//         }
//         print!("\nPress <enter> to retry ...");
//         stdout().flush()?;
//         input.clear();
//         stdin().read_line(&mut input)?;
//         println!("\n");
//     }

//     Ok(())
// }
