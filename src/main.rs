use std::collections::HashMap;

use clap::{App, Arg, ArgMatches};
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct WebhookTrigger {
    id: String,
    labels: HashMap<String, String>,
}

//
// run the CLI as follows:
// fp webhook prometheus wht-id-1234567 --label dev=sda1 --label instance=127.0.0.1 --annotation example=foobar
fn main() {
    let matches = App::new("Fiberplane CLI")
        .version("0.1")
        .author("The Fiberplane Team")
        .about("Interacts with the Fiberplane API")
        .subcommand(
            App::new("webhook")
                .about("Interact with Fiberplane Webhooks")
                .arg(
                    Arg::new("label")
                        .short('l')
                        .long("label")
                        .multiple(true)
                        .about("Sets the alert labels"),
                )
                .arg(
                    Arg::new("annotation")
                        .short('a')
                        .long("annotation")
                        .multiple(true)
                        .about("Set the alert annotations"),
                ),
        )
        .get_matches();

    println!("{:?}", matches);

    // match app_m.subcommand() {
    //     ("clone",  Some(sub_m)) => {}, // clone was used
    //     ("push",   Some(sub_m)) => {}, // push was used
    //     ("commit", Some(sub_m)) => {}, // commit was used
    //     _                       => {}, // Either no subcommand or one not tested for...
    // }

    match matches.subcommand() {
        Some(("webhook", sub_m)) => handle_webhook(sub_m),
        //Some(("other command", sub_m)) => do_something_else(sub_m),
        _ => panic!(),
    }
}

fn handle_webhook(matches: &ArgMatches) {
    println!("{:?}", matches);

    let label_args: Vec<_> = matches.values_of("label").unwrap().collect();

    let mut labels: HashMap<String, String> = HashMap::new();

    for l in label_args {
        let vec: Vec<&str> = l.split("=").collect();
        println!("{:?}", vec);
        labels.insert(vec[0].to_string(), vec[1].to_string());
    }

    let wht = WebhookTrigger {
        id: "amazing webhook id".to_string(),
        labels: labels,
    };

    println!("{:?}", wht);
    let result = do_request(wht);
    //println!("{:?}", result.await?);
}

async fn do_request(wht: WebhookTrigger) -> Result<(), reqwest::Error> {
    let response = Client::new()
        .post("https://dev.fiberplane.io")
        .json(&wht)
        .send()
        .await?
        .json()
        .await?;

    println!("{:?}", response);
    Ok(())
}
