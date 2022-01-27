#![no_main]
#![no_std]
#![feature(asm)]

use panic_halt as _;

use cortex_m_rt::entry;
use fourtris::game::{Game, GameState, Input};
use fourtris::game_renderer::GameRenderer;
//use cortex_m_semihosting::{debug, hprintln};

mod lcd_backend;
use lcd_backend::{Lcd, LcdBackend};

mod randy;
use randy::Randy;

mod timer;
use timer::Timer0;


// initialize the ADCs for reading the XY values for the joystick.
fn initialize_adcs(p: &tm4c123x::Peripherals) {
    // 1. enable both ADC modules. One will be used to sample the X axis and one will be used to
    //    sample the y-axis
    p.SYSCTL.rcgcadc.modify(|r, w| unsafe { w.bits( r.bits() | 3 ) } );
    // 2. enable clock for the B and D ports                      DCBA
    p.SYSCTL.rcgcgpio.modify(|r, w| unsafe { w.bits (r.bits() | 0b1010) } );

    // wait for port B and D to be ready for use
    while p.SYSCTL.prgpio.read().bits() & 0b1010 != 0b1010 {}

    // 3. set AFSEL bits for PB5 and PD3                               54 3210
    p.GPIO_PORTB_AHB.afsel.modify(|r, w| unsafe { w.bits( r.bits() | 0b10_0000 ) }  );
    //                                                                 3210
    p.GPIO_PORTD_AHB.afsel.modify(|r, w| unsafe { w.bits( r.bits() | 0b1000 ) }  );
    // 4. configure PB5 and PD3 as analog by clearing GPIODEN bits
    p.GPIO_PORTB_AHB.den.modify(|r, w| unsafe { w.bits( r.bits() & (!0b10_0000) ) } );
    p.GPIO_PORTD_AHB.den.modify(|r, w| unsafe { w.bits( r.bits() & (!0b1000) ) } );
    // 5. disable analog isolation circuit for the ADC input pins (PB5 and PD3)
    p.GPIO_PORTB_AHB.amsel.modify(|r, w| unsafe { w.bits( r.bits() | 0b10_0000 ) }  );
    p.GPIO_PORTD_AHB.amsel.modify(|r, w| unsafe { w.bits( r.bits() | 0b1000 ) }  );
    // 7. disable sample sequencer 0 in both ADCs
    // we will sample a different axis on both
    p.ADC0.actss.modify(|r, w| unsafe { w.bits( r.bits() & (!1) ) } );
    p.ADC1.actss.modify(|r, w| unsafe { w.bits( r.bits() & (!1) ) } );
    // 8. configure trigger event for the sample sequencer
    //    we are using ss0, and will use continuous sampling
    p.ADC0.emux.modify(|r, w| unsafe { w.bits( r.bits() | 0xF ) } );
    p.ADC1.emux.modify(|r, w| unsafe { w.bits( r.bits() | 0xF ) } );
    // 10. configure input source
    //     ADC0 will sample from PB5 (AIN 11)
    //     ADC1 will sample from PD3 (AIN 4)
    p.ADC0.ssmux0.modify(|r, w| unsafe { w.bits( (r.bits() & (!0xF)) | 11) } );
    p.ADC1.ssmux0.modify(|r, w| unsafe { w.bits( (r.bits() & (!0xF)) | 4) } );
    // 11. configure the sample control bits in the control registers
    //     0010
    //     read from pin, no interrupt, end of sequence, not differential input
    //     only one sample from the sequence
    p.ADC0.ssctl0.write(|w| unsafe { w.bits(2) });
    p.ADC1.ssctl0.write(|w| unsafe { w.bits(2) });
    // 13. enable sample sequencer 0 on both ADCs
    p.ADC0.actss.modify(|r, w| unsafe { w.bits( r.bits() | 1 ) } );
    p.ADC1.actss.modify(|r, w| unsafe { w.bits( r.bits() | 1 ) } );

    // wait for the ADCs to be ready
    while p.SYSCTL.pradc.read().bits() & 0b11 != 0b11 {}
}

// Configure PD6, PD7, and PE4 as inputs for the push buttons and joystick
fn initialize_buttons(p: &tm4c123x::Peripherals) {
    // --------------------------------------------------
    // initialize GPIO pins for push buttons and joystick
    // These buttons pull the pins to ground
    // there are pull-up resistors on the pcb
    // --------------------------------------------------
    // 1. enable clock for ports D and E (bits 3 and 4)                     E DBCA
    p.SYSCTL.rcgcgpio.modify(|r, w| unsafe { w.bits( r.bits() | 0b1_1000 )});

    // wait for ports D and E to be ready
    while p.SYSCTL.prgpio.read().bits() & 0b1_1000 != 0b1_1000 {}
    // unlock the GPIOCR register, and modify it so we can configure PD7
    p.GPIO_PORTD_AHB.lock.write(|w| unsafe { w.bits( 0x4C4F434B ) });
    p.GPIO_PORTD_AHB.cr.modify(|r, w| unsafe { w.bits( r.bits() | 0x80 ) });
    // 2. set pins D6 and D7 as input                                            7654 3210
    p.GPIO_PORTD_AHB.dir.modify(|r, w| unsafe { w.bits( r.bits() & (!0b1100_0000) )});
    p.GPIO_PORTD_AHB.den.modify(|r, w| unsafe { w.bits( r.bits() |   0b1100_0000)});
    // 3. set pin E4 as input                                                    4 3210
    p.GPIO_PORTE_AHB.dir.modify(|r, w| unsafe { w.bits( r.bits() & (!0b1_0000) )});
    p.GPIO_PORTE_AHB.den.modify(|r, w| unsafe { w.bits( r.bits() | 0b1_0000 )});
}

// Initialize the random number generator
fn initialize_rng(rng: &mut Randy, p: &tm4c123x::Peripherals) {
    while rng.nums_available() < rng.capacity() {
        // wait for data to be available (X-axis)
        while p.ADC0.ssfstat0.read().bits() & 0x100 != 0 {}
        // wait for data to be available (Y-axis)
        while p.ADC1.ssfstat0.read().bits() & 0x100 != 0 {}

        let x_data = p.ADC0.ssfifo0.read().bits() & 1;
        let y_data = p.ADC1.ssfifo0.read().bits() & 1;

        rng.add_bit((x_data ^ y_data) as usize);
    }
}

#[entry]
fn main() -> ! {
    let peripherals = tm4c123x::Peripherals::take().unwrap();

    // set ports A, B, D, E and F to use the fast GPIO bus
    // 0b100011
    // 0b111011
    //   FEDCBA
    peripherals.SYSCTL.gpiohbctl.modify(|r, w| unsafe { w.bits (r.bits() | 0b11_1011) } );


    initialize_adcs(&peripherals);
    initialize_buttons(&peripherals);

    let mut rng = Randy::new();
    initialize_rng(&mut rng, &peripherals);

    // CONFIGURE THE TIMER!
    // want to run at 60fps
    // clock is 16 MHz
    // timer value = 16_000_000 / 60 ~ 266_667
    // eh, turns out 70 fps is more fun! :)
    let timer0 = Timer0::new(&peripherals, 16000000 / 70);

    // initialize the LCD
    let mut lcd_backend = LcdBackend::new(Lcd::new(&peripherals));


    let mut game = Game::new(&mut rng);
    let mut input : Input = Default::default();
    let mut state = GameState::Playing;

    lcd_backend.draw_initial_screen();
    lcd_backend.turn_on_display();
    
    timer0.start();
    loop {
        // Check to see if the user wants to restart the game
        let porte = peripherals.GPIO_PORTE_AHB.data.read().bits();

        let reset_selected = porte & 0x10 == 0x00;

        // joystick select resets the game
        if reset_selected {
            game = Game::new(&mut rng);
            // clear screen
            lcd_backend.clear_playing_field();
            lcd_backend.draw_score(0);
            lcd_backend.draw_level(1);
            state = GameState::Playing;
        }

        // Check ADC data, and use it to get some more "random" bits
        let mut random_bit = 0;
        // get input
        // see if there is a sample ready for ADC0 - X axis
        let horizontal_data_available = peripherals.ADC0.ssfstat0.read().bits() & 0x100 == 0;
        if horizontal_data_available {
            // reset horizontal inputs
            input.left = false;
            input.right = false;

            let horizontal_reading = peripherals.ADC0.ssfifo0.read().bits();
            if horizontal_reading < 50 {
                input.left = true;
            } else if horizontal_reading > 4000 {
                input.right = true;
            }

            random_bit = horizontal_reading;
        }

        // see if there is a sample ready for ADC1 - Y axis
        let vertical_data_available = peripherals.ADC1.ssfstat0.read().bits() & 0x100 == 0;
        if vertical_data_available {
            let vertical_reading = peripherals.ADC1.ssfifo0.read().bits();
            input.down = vertical_reading < 10;

            random_bit ^= vertical_reading;
        }

        // use ADC readings to generate a "random" bit
        // and add it to our random number generator
        if horizontal_data_available && vertical_data_available {
            rng.add_bit((random_bit & 1) as usize);
        }

        // button 1 (PD6) is for counter clockwise rotation
        // button 2 (PD7) is for counter clockwise rotation
        let portd = peripherals.GPIO_PORTD_AHB.data.read().bits();
        
        let ccw_rotate = portd & 0b1000_0000 == 0; 
        let cw_rotate  = portd & 0b0100_0000 == 0;

        input.ccw_rotate = ccw_rotate;
        input.cw_rotate = cw_rotate;

        match state {
            GameState::Playing => {
                // process input
                state = game.run_loop(&input, &mut rng);
                // draw to the screen
                game.draw(&mut lcd_backend);
            },
            GameState::GameOver => {
            },
        }

        // chill out until a timer interrupt occurs
        while !timer0.timeout_occured() {}
        timer0.clear_interrupt();
    }
}
