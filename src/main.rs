use clap::{App, Arg, ArgMatches};

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

    // read config file

    // create trigger crud

    // 1) read cli args
    // 2) (turn into alert)
    // 3) do POST on api/webhooks/
    
}

fn handle_webhook(matches: &ArgMatches) {
    println!("Hello, world!");
    //turn this into a struct
    // do the request
    println!("{:?}", matches);

    let labels: Vec<_> = matches.values_of("label").unwrap().collect();
    let vec: Vec<&str>;// = split.collect();
    for l in labels {
        l.split("=");
        vec.collect();
        //println!("{}", l);
    }
    let vec: Vec<&str> = split.collect();
    
    //println!("{:?}", x);
    //println!("{:?}", matches.value_of("label"));

}
