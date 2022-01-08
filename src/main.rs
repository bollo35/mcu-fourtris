#![no_main]
#![no_std]
#![feature(asm)]

use panic_halt as _;

use cortex_m_rt::entry;
use fourtris::game::{Game, GameState, Input};
use fourtris::game_renderer::{TetriminoType, GameRenderer};
use fourtris::rng::Rng;
//use cortex_m_semihosting::{debug, hprintln};


// Pins used for the LCD
// PA4 - CS (Chip select)
// PB4 - SPI CLK
// PB7 - SSI2Tx (MOSI)
// PF0 - !RESET
// PF3 - Backlight UNUSED (only would need to be enabled if you had the jumper set appropriately)
// PF4 - D/CX (data/command)
#[entry]
fn main() -> ! {
    let peripherals = tm4c123x::Peripherals::take().unwrap();

    // set ports A, B, D, E and F to use the fast GPIO bus
    // 0b100011
    // 0b111011
    //   FEDCBA
    peripherals.SYSCTL.gpiohbctl.modify(|r, w| unsafe { w.bits (r.bits() | 0x3B) } );

    let mut lcd = LcdBackend::new(&peripherals);

    // --------------------------------------
    //    initialize and configure the ADCs
    // --------------------------------------
    // 1. enable both ADC modules. One will be used to sample the X axis and one will be used to
    //    sample the y-axis
    peripherals.SYSCTL.rcgcadc.modify(|r, w| unsafe { w.bits( r.bits() | 3 ) } );
    // 2. enable clock for the B and D ports
    peripherals.SYSCTL.rcgcgpio.modify(|r, w| unsafe { w.bits (r.bits() | 0x0A) } );
    // 3. set AFSEL bits for PB5 and PD3
    peripherals.GPIO_PORTB_AHB.afsel.modify(|r, w| unsafe { w.bits( r.bits() | 0x20 ) }  );
    peripherals.GPIO_PORTD_AHB.afsel.modify(|r, w| unsafe { w.bits( r.bits() | 8 ) }  );
    // 4. configure PB5 and PD3 as analog by clearing GPIODEN bits
    peripherals.GPIO_PORTB_AHB.den.modify(|r, w| unsafe { w.bits( r.bits() & (!0x20) ) } );
    peripherals.GPIO_PORTD_AHB.den.modify(|r, w| unsafe { w.bits( r.bits() & (!8) ) } );
    // 5. disable analog isolation circuit for the ADC input pins (PB5 and PD3)
    peripherals.GPIO_PORTB_AHB.amsel.modify(|r, w| unsafe { w.bits( r.bits() | 0x20 ) }  );
    peripherals.GPIO_PORTD_AHB.amsel.modify(|r, w| unsafe { w.bits( r.bits() | 8 ) }  );
    // 7. disable sample sequencer 0 in both ADCs
    // we will sample a different axis on both
    peripherals.ADC0.actss.modify(|r, w| unsafe { w.bits( r.bits() & (!1) ) } );
    peripherals.ADC1.actss.modify(|r, w| unsafe { w.bits( r.bits() & (!1) ) } );
    // 8. configure trigger event for the sample sequencer
    //    we are using ss0, and will use continuous sampling
    peripherals.ADC0.emux.modify(|r, w| unsafe { w.bits( r.bits() | 0xF ) } );
    peripherals.ADC1.emux.modify(|r, w| unsafe { w.bits( r.bits() | 0xF ) } );
    // 10. configure input source
    //     ADC0 will sample from PB5 (AIN 11)
    //     ADC1 will sample from PD3 (AIN 4)
    peripherals.ADC0.ssmux0.modify(|r, w| unsafe { w.bits( (r.bits() & (!0xF)) | 11) } );
    peripherals.ADC1.ssmux0.modify(|r, w| unsafe { w.bits( (r.bits() & (!0xF)) | 4) } );
    // 11. configure the sample control bits in the control registers
    //     0010
    //     read from pin, no interrupt, end of sequence, not differential input
    //     only one sample from the sequence
    peripherals.ADC0.ssctl0.write(|w| unsafe { w.bits(2) });
    peripherals.ADC1.ssctl0.write(|w| unsafe { w.bits(2) });
    // 13. enable sample sequencer 0 on both ADCs
    peripherals.ADC0.actss.modify(|r, w| unsafe { w.bits( r.bits() | 1 ) } );
    peripherals.ADC1.actss.modify(|r, w| unsafe { w.bits( r.bits() | 1 ) } );

    // --------------------------------------------------
    // initialize GPIO pins for push buttons and joystick
    // These buttons pull the pins to ground
    // there are pull-up resistors on the pcb
    // --------------------------------------------------
    // 1. enable clock for ports D and E (bits 3 and 4)
    peripherals.SYSCTL.rcgcgpio.modify(|r, w| unsafe { w.bits( r.bits() | 0x18 )});
    // unlock the GPIOCR register, and modify it so we can configure PD7
    peripherals.GPIO_PORTD_AHB.lock.write(|w| unsafe { w.bits( 0x4C4F434B ) });
    peripherals.GPIO_PORTD_AHB.cr.modify(|r, w| unsafe { w.bits( r.bits() | 0x80 ) });
    // 2. set pins D6 and D7 as input
    peripherals.GPIO_PORTD_AHB.dir.modify(|r, w| unsafe { w.bits( r.bits() & (!0xC0) )});
    peripherals.GPIO_PORTD_AHB.den.modify(|r, w| unsafe { w.bits( r.bits() | 0xC0 )});
    // 3. set pin E4 as input
    peripherals.GPIO_PORTE_AHB.dir.modify(|r, w| unsafe { w.bits( r.bits() & (!0x10) )});
    peripherals.GPIO_PORTE_AHB.den.modify(|r, w| unsafe { w.bits( r.bits() | 0x10 )});

    // ---------------------------------------
    //   RANDOM NUMBER HOLDER INITIALIZATION
    // ---------------------------------------
    // initialize our random number holder and get some data
    peripherals.GPIO_PORTF_AHB.data.modify(|r, w| unsafe { w.bits( r.bits() | 8) });
    let mut rng = Randy::new();

    while rng.nums_available() < BUF_SIZE {
        // wait for data to be available (X-axis)
        while peripherals.ADC0.ssfstat0.read().bits() & 0x100 != 0 {}
        // wait for data to be available (Y-axis)
        while peripherals.ADC1.ssfstat0.read().bits() & 0x100 != 0 {}

        let x_data = peripherals.ADC0.ssfifo0.read().bits() & 1;
        let y_data = peripherals.ADC1.ssfifo0.read().bits() & 1;

        rng.add_bit((x_data ^ y_data) as usize);
    }
    peripherals.GPIO_PORTF_AHB.data.modify(|r, w| unsafe { w.bits( r.bits() & (!8) ) });



    let mut game = Game::new(&mut rng);
    let mut input : Input = Default::default();
    let mut state = GameState::Playing;

    loop {
        // check if we don't have enough random data
        if rng.nums_available() < 6 {
            peripherals.GPIO_PORTF_AHB.data.modify(|r, w| unsafe { w.bits( r.bits() | 8) });
        } else {
            peripherals.GPIO_PORTF_AHB.data.modify(|r, w| unsafe { w.bits( r.bits() & (!8) ) });
        }

        // Check to see if the user wants to restart the game
        let porte = peripherals.GPIO_PORTE_AHB.data.read().bits();
        // joystick select resets the game
        if porte & 0x10 == 0x00 {
            game = Game::new(&mut rng);
            // clear screen
            lcd.clear_playing_field();
            lcd.draw_score(0);
            lcd.draw_level(1);
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
            input.down = vertical_reading < 50;

            random_bit ^= vertical_reading;
        }

        if horizontal_data_available && vertical_data_available {
            rng.add_bit((random_bit & 1) as usize);
        }

        // button 1 (PD6) is for counter clockwise rotation
        // button 2 (PD7) is for counter clockwise rotation
        let portd = peripherals.GPIO_PORTD_AHB.data.read().bits();
        let ccw_rotate = portd & 0x80 == 0; 
        let cw_rotate = portd & 0x40 == 0;

        input.ccw_rotate = ccw_rotate;
        input.cw_rotate = cw_rotate;

        match state {
            GameState::Playing => {
                state = game.run_loop(&input, &mut rng);
                // draw to the screen
                game.draw(&mut lcd);
            },
            GameState::GameOver => {
            },
        }
    }
}

#[repr(u8)]
pub enum LcdCommand {
    SWRESET = 0x01,
    SLPOUT  = 0x11,
    NORON   = 0x13,
    INVOFF  = 0x20,
    DISPON  = 0x29,
    CASET   = 0x2A,
    RASET   = 0x2B,
    RAMWR   = 0x2C,
    MADCTL  = 0x36,
    COLMOD  = 0x3A,
    FRMCTR1 = 0xB1,
    FRMCTR2 = 0xB2,
    FRMCTR3 = 0xB3,
    INVCTR  = 0xB4,
    PWCTR1  = 0xC0,
    PWCTR2  = 0xC1,
    PWCTR3  = 0xC2,
    PWCTR4  = 0xC3,
    PWCTR5  = 0xC4,
    VMCTR1  = 0xC5,
    GMCTRP1 = 0xE0,
    GMCTRN1 = 0xE1,
}

pub fn long_delay() {
    for _ in 0..5000000 {
        unsafe {
            asm!("nop");
        }
    }
}

pub fn mini_delay() {
    for _ in 0..100000 {
        unsafe {
            asm!("nop");
        }
    }
}

pub struct LcdBackend<'a> {
    p: &'a tm4c123x::Peripherals,
    repeat_count: usize,
}

impl LcdBackend<'_> {
    pub fn new(p: &tm4c123x::Peripherals) -> LcdBackend {
        // this is to determine how many iterations of our 2 pixels worth of data to write to the
        // lcd screen
        let mut lcd_backend = LcdBackend {
            p: p,
            repeat_count: ((BLOCK_WIDTH * BLOCK_WIDTH) >> 1) as usize,
        };

        // --------------------------------------
        // HARDWARE INITIALIZATION
        // --------------------------------------
        // port A and F setup - A4, F0, F3, F4 are needed as GPIO outputs
        // --------------------------------------
        // 1. enable clock for port A and F GPIO pins
        p.SYSCTL.rcgcgpio.modify(|r, w| unsafe { w.bits (r.bits() | 0x21) } );
        // --------------------
        // --- PORT A setup ---
        // --------------------
        // 2. set PA4 as an output
        p.GPIO_PORTA_AHB.dir.modify(|r,w| unsafe { w.bits(r.bits() | 0x10) });
        // 3. enable pullup resistor on PA4. Default value should be high
        p.GPIO_PORTA_AHB.pur.modify(|r, w| unsafe { w.bits( r.bits() | 0x10) });
        // 4. enable digital output on PA4
        p.GPIO_PORTA_AHB.den.write(|w| unsafe { w.bits(0x10) });
        // --------------------
        // --- PORT F setup ---
        // --------------------
        // 5. set PF0, PF3, and PF4 as outputs
        p.GPIO_PORTF_AHB.dir.modify(|r, w| unsafe { w.bits( r.bits() | 0x19 ) }); 
        // 3. enable pullup resistor on PF0. Default value should be high
        p.GPIO_PORTF_AHB.pur.modify(|r, w| unsafe { w.bits( r.bits() | 1) });
        // 6. enable digital output on PF0, PF3, and PF4 as outputs
        p.GPIO_PORTF_AHB.den.modify(|r, w| unsafe { w.bits( r.bits() | 0x19 ) });

        // SET INITIAL PIN STATES
        /*
        // set Backlight (PF3)  and !RESET (PF0) high - MODIFIED should OR with 9 to do both
        // Backlight can be jumpered on the board, if you move the jumper, then change this to 9
        peripherals.GPIO_PORTF_AHB.data.modify(|r, w| unsafe { w.bits( r.bits() | 1) });
        */
        // set CS (PA4) low
        lcd_backend.cs_low();
        // set !RESET (PF0) low
        lcd_backend.reset_low();
        // set D/CX (PF4) low
        lcd_backend.dcx_low();

        // --------------------------------------
        //    initialize and configure the SSI2
        // --------------------------------------
        // 1. enable ssi2 - 3rd bit enables SSI2
        p.SYSCTL.rcgcssi.modify(|r, w| unsafe { w.bits( r.bits() | 4 ) });
        // 2. enable clock for port B GPIO pins
        p.SYSCTL.rcgcgpio.modify(|r, w| unsafe { w.bits (r.bits() | 2) } );
        // 3. enable alternative functions for pins 4 and 7
        p.GPIO_PORTB_AHB.afsel.modify(|r, w| unsafe { w.bits( (1<<7) | (1<<4) | r.bits() ) });
        // 4. set appropriate PMC bits in GPIOCTL
        p.GPIO_PORTB_AHB.pctl.modify(|r, w| unsafe { w.bits( (2<<28) | (2<<16) | r.bits() ) });
        // 5. Enable digital outputs
        p.GPIO_PORTB_AHB.den.modify(|r, w| unsafe { w.bits( (1<<7) | (1<<4) | r.bits() ) });
        // 6. clear SSICR1 to disable SSI2 module, so we can configure it
        // Set master mode and SSE (bits 1 and 2)
        p.SSI2.cr1.modify(|r, w| unsafe { w.bits(r.bits() & (!6) ) });
        // 7. set clock source - 0 = system clock, 5 = PIOSC (no idea what that is yet)
        p.SSI2.cc.modify(|r, w| unsafe { w.bits( (r.bits() & (!0xF)) | 0 /*5*/) });
        // 8. set prescale divisor (must be an even number between 2 and 254)
        p.SSI2.cpsr.modify(|r,w| unsafe { w.bits( (r.bits() & (!0xFF)) |  4) });
        // 9. write to CR0 - serial clock rate (SCR), SPH, SPO, Protocol mode, Data size
        //  upper nibble is [SPH (1bit) | SPO (1bit) | FRF (2bits)]
        //  lower nibble is the data size HAHHAHAHAHAHAH i made it 9 bit data!!!!
        p.SSI2.cr0.modify(|r, w| unsafe { w.bits( (r.bits() & !(0xFF))  | 7) });
        // 10. (optional) enable uDMA

        // 11. enable SSI2 module
        p.SSI2.cr1.modify(|r, w| unsafe { w.bits (r.bits() | 2) });

        // -------------------------------
        // CONFIGURE THE LCD FOR OPERATION
        // -------------------------------
        mini_delay();
        lcd_backend.reset_high();
        // set CS (PA4) high
        lcd_backend.cs_high();

        mini_delay();

        // ** SLPOUT command
        lcd_backend.write_command(LcdCommand::SLPOUT);
        mini_delay();

        // ** set color mod: rgb 4-4-4
        let colmod_param = [3];
        lcd_backend.write_command(LcdCommand::COLMOD);
        lcd_backend.write_data(&colmod_param[..], 0);

        // ** MADCTL
        lcd_backend.write_command(LcdCommand::MADCTL);
        // 0b00001000 -> set the pixel data order to RGB.
        // 0b00110000 -> set the (0,0) coord to the top-left of the LCD screen
        lcd_backend.write_data(&[0xC8], 0);

        // draw the initial screen
        // COLUMN_START = 2
        // ROW_START = 3
        // 1. clear screen to white
        lcd_backend.set_drawing_area(0, 0, 128, 128);
        lcd_backend.write_pixels_repeatedly(&[0xFF, 0xFF, 0xFF], 8192);

        // 2. make playing field black
        lcd_backend.set_drawing_area(PLAYFIELD_HORIZONTAL_PADDING as u8,
                                     PLAYFIELD_VERTICAL_PADDING as u8,
                                     10 * 5, // BLOCK_WIDTH = 5
                                     22 * 5); // BLOCK_WIDTH = 5
        lcd_backend.write_pixels_repeatedly(&[0x00, 0x00, 0x00], 2750);
        // 3. display the level on the left side
        // level text is 25 pixels wide, we have 39 pixels to work with
        // we wish to center the text so, the start pixel for the text should be:
        // (39 - 25) / 2 = 7
        lcd_backend.set_drawing_area(7,
                                     PLAYFIELD_VERTICAL_PADDING as u8, // make it line up with the top of the playing field
                                     LEVEL_TEXT_WIDTH + 1, // added an extra space since we wanted to use an even amount of pixels to specify it
                                     TEXT_HEIGHT); // 8 pixels high
        lcd_backend.write_pixels(&LEVEL_TEXT);
        lcd_backend.draw_level(1);
        // 4. display the score on the right side
        // score text is 24 pixels wide, we have 39 pixels to work with
        // (39 - 24) / 2 = 7 (integer math!)
        // 7 +  39 (padding on left side) + 50 (playing field width) = 98
        lcd_backend.set_drawing_area(7 + PLAYFIELD_HORIZONTAL_PADDING + PLAYFIELD_WIDTH,
                                     PLAYFIELD_VERTICAL_PADDING as u8,
                                     SCORE_TEXT_WIDTH, // 24 pixels wide (no trickery this time)
                                     TEXT_HEIGHT); // 8 pixels high
        lcd_backend.write_pixels(&SCORE_TEXT);
        lcd_backend.draw_score(0);

        // ** turn the display on
        lcd_backend.write_command(LcdCommand::DISPON);
        // wait at least 120 ms
        mini_delay();

        // return the struct for use
        lcd_backend
    }

    // -----------------------------------------------
    //  Convenience functions for important GPIO pins
    // -----------------------------------------------
    fn cs_high(&self) {
        // set CS (PA4) high
        self.p.GPIO_PORTA_AHB.data.modify(|r, w| unsafe { w.bits( r.bits() | 0x10 ) });
    }

    fn cs_low(&self) {
        // set CS (PA4) low
        self.p.GPIO_PORTA_AHB.data.modify(|r, w| unsafe { w.bits( r.bits() & (!0x10) ) });
    }

    fn reset_high(&self) {
        // set !RESET (PF0) high
        self.p.GPIO_PORTF_AHB.data.modify(|r, w| unsafe { w.bits( r.bits() | 1 ) });
    }

    fn reset_low(&self) {
        // set !RESET (PF0) low
        self.p.GPIO_PORTF_AHB.data.modify(|r, w| unsafe { w.bits( r.bits() & !1 ) });
    }
    
    fn dcx_high(&self) {
        // set D/CX (PF4) low
        self.p.GPIO_PORTF_AHB.data.modify(|r, w| unsafe { w.bits( r.bits() | 0x10) });
    }

    fn dcx_low(&self) {
        // set D/CX (PF4) low
        self.p.GPIO_PORTF_AHB.data.modify(|r, w| unsafe { w.bits( r.bits() & (!0x10)) });
    }

    const SSI_MODULE_BUSY : u32 = 0x10;
    /// Returns true when the SSI module is transmitting data (TODO: double check the data sheet)
    fn is_ssi_busy(&self) -> bool {
        self.p.SSI2.sr.read().bits() & LcdBackend::SSI_MODULE_BUSY == LcdBackend::SSI_MODULE_BUSY
    }

    //--------------------------------------------------
    // FUNDAMENTAL FUNCTIONS FOR COMMUNICATING WITH LCD
    //--------------------------------------------------
    fn write_command(&self, cmd: LcdCommand) {
        // Don't do anything if the module is currently busy
        while self.is_ssi_busy() {}
        // 1. set D/CX (PF4) low
        self.dcx_low();
    
        // 2. set CS (PA4) low
        self.cs_low();
    
        // 3. send data
        self.p.SSI2.dr.write(|w| unsafe { w.bits(cmd as u32) });
    
        // wait for data to finish transmitting
        while self.is_ssi_busy() {}
    
        // 4. set CS (PA4) high
        self.cs_high();
    }

    // repeat parameter is just a "hack" for this test
    // or is it?
    fn write_data(&self, data: &[u8], repeat: usize) {
        // Don't do anything if the module is currently busy
        while self.is_ssi_busy() {}
    
        let iterations = repeat + 1;
        for _ in 0..iterations {
            for c in data {
    
                // 1. set D/CX (PF4) high
                self.dcx_high();
    
                // 2. set CS (PA4) low
                self.cs_low();
    
                // send data
                self.p.SSI2.dr.write(|w| unsafe { w.bits(*c as u32) });
    
                // wait for transmission to complete
                while self.is_ssi_busy() {}
    
                // 4. set CS (PA4) high
                self.cs_high();
            }
        }
    }

    // HIGHER ABSTRACTIONS USED internally
    fn set_drawing_area(&self, x: u8, y: u8, width: u8, height: u8) {
        // ** set column range
        self.write_command(LcdCommand::CASET);
        self.write_data(&[0, 2 + x, 0, 2 + x + width - 1], 0);

        // ** set row range
        self.write_command(LcdCommand::RASET);
        self.write_data(&[0, 3 + y, 0, 3 + y + height - 1], 0);
    }

    pub fn write_pixels(&self, data: &[u8]) {
        self.write_command(LcdCommand::RAMWR);
        self.write_data(data, 0);
    }

    pub fn write_pixels_repeatedly(&self, data: &[u8], repeat: usize) {
        self.write_command(LcdCommand::RAMWR);
        self.write_data(data, repeat);
    }

    // FINALLY the consumer functions
    pub fn clear_playing_field(&self) {
        // make the playing field black
        self.set_drawing_area(PLAYFIELD_HORIZONTAL_PADDING as u8,
                           PLAYFIELD_VERTICAL_PADDING as u8,
                           PLAYFIELD_WIDTH as u8,
                           PLAYFIELD_HEIGHT as u8);
        self.write_pixels_repeatedly(&[0x00, 0x00, 0x00], 2750);
    }

}

impl GameRenderer for LcdBackend<'_> {
    fn draw_block(&mut self, x: u8, y: u8, tetrimino_type: TetriminoType) {
        let pixel_data = 
            match tetrimino_type {
                TetriminoType::I => {
                    // blue
                    [0x00, 0xF0, 0x0F]
                },
                TetriminoType::O => {
                    // green
                    [0x0A, 0x00, 0xA0]
                },
                TetriminoType::J => {
                    // cyan
                    [0x0A, 0xA0, 0xAA]
                },
                TetriminoType::L => {
                    // red
                    [0xF0, 0x0F, 0x00]
                },
                TetriminoType::S => {
                    // purple
                    [0xA0, 0xAA, 0x0A]
                },
                TetriminoType::Z => {
                    // yellow
                    [0xAA, 0x0A, 0xA0]
                    //[0x77, 0x07, 0x70]
                },
                TetriminoType::T => {
                    // lime green
                    [0x7F, 0x77, 0xF7]
                },
                TetriminoType::EmptySpace => {
                    // black
                    [0x00, 0x00, 0x00]
                },

                /*
                TetriminoType::LiveTetrimino => {
                    // blue
                    [0x00, 0xF0, 0x0F]
                },
                TetriminoType::SettledTetrimino => {
                    // gray
                    [0x77, 0x77, 0x77]
                },
                TetriminoType::EmptySpace => {
                    // black
                    [0x00, 0x00, 0x00]

                }
                */
            };

        let xs = (x as u8)*BLOCK_WIDTH + PLAYFIELD_HORIZONTAL_PADDING;
        let ys = (y as u8)*BLOCK_WIDTH + PLAYFIELD_VERTICAL_PADDING;
        self.set_drawing_area(xs, ys, BLOCK_WIDTH, BLOCK_WIDTH);

        // ** write some data to ram
        self.write_pixels_repeatedly(&pixel_data[..], self.repeat_count);
    }

    fn draw_score(&mut self, score: u32) {
        // erase the old score displayed
        self.set_drawing_area(7 + PLAYFIELD_HORIZONTAL_PADDING + PLAYFIELD_WIDTH,
                              12 + PLAYFIELD_VERTICAL_PADDING as u8,
                              SCORE_TEXT_WIDTH,
                              TEXT_HEIGHT);
        self.write_pixels_repeatedly(&[0xFF, 0xFF, 0xFF], 96);
        // cap the displayed score at 999 because...we can't allow people to brag about their score
        // too much...or something
        let mut displayed_score = if score > 999 { 999 } else { score };
        let num_digits =
            if score > 99 {
                3
            } else if score > 9 {
                2
            } else {
                1
            };
        let display_width = 5 * num_digits - 1; // 4 pixels per digit + 1 space between each number when necessary
                                                // 4 * #digits        + (#digits - 1)
                                                // -1 because we don't need a space after the last digit
        // start at the centerpoint of the score text + our half width - 4
        // whaaat? that gives us the location of the last digit
        let mut display_start = 7 + PLAYFIELD_HORIZONTAL_PADDING +
                                    PLAYFIELD_WIDTH +
                                    SCORE_TEXT_WIDTH/2 +
                                    display_width/2 - 4;
        for _ in 0..num_digits {
            let digit = (displayed_score % 10) as usize;
            self.set_drawing_area(display_start,
                                  12 + PLAYFIELD_VERTICAL_PADDING as u8, // give some space between SCORE text and #s
                                  NUMBER_CHAR_WIDTH,  // 4 pixels wide (no trickery this time)
                                  TEXT_HEIGHT); // 8 pixels high
            self.write_pixels(&NUMBER_TEXT[digit*BYTES_IN_NUM_TEXT..(digit+1)*BYTES_IN_NUM_TEXT]);
            display_start -= 5;
            displayed_score /= 10;
        }
    }


    fn draw_level(&mut self, level: usize) {
        // erase the old level displayed
        self.set_drawing_area(7,
                              12 + PLAYFIELD_VERTICAL_PADDING as u8,
                              LEVEL_TEXT_WIDTH,
                              TEXT_HEIGHT);
        self.write_pixels_repeatedly(&[0xFF, 0xFF, 0xFF], 96);
        let num_digits =
            if level > 9 {
                2
            } else {
                1
            };
        let display_width = 5 * num_digits - 1; // 4 pixels per digit + 1 space between each number when necessary
                                                // 4 * #digits        + (#digits - 1)
                                                // -1 because we don't need a space after the last digit
        // start at the centerpoint of the score text + our half width - 4
        // whaaat? that gives us the location of the last digit
        let mut display_start = 7 + SCORE_TEXT_WIDTH/2 + display_width/2 - NUMBER_CHAR_WIDTH;
        let mut level_copy = level;
        for _ in 0..num_digits {
            let digit = (level_copy % 10) as usize;
            self.set_drawing_area(display_start,
                                  12 + PLAYFIELD_VERTICAL_PADDING as u8, // give some space between SCORE text and #s
                                  NUMBER_CHAR_WIDTH,  // 4 pixels wide (no trickery this time)
                                  TEXT_HEIGHT); // 8 pixels high
            self.write_pixels(&NUMBER_TEXT[digit*BYTES_IN_NUM_TEXT..(digit+1)*BYTES_IN_NUM_TEXT]);
            display_start -= 5;
            level_copy /= 10;
        }
    }
}

pub struct Randy {
    buf: [usize; BUF_SIZE],
    head: usize,
    tail: usize,
    candidate: usize,
    bits_accumulated: usize,
    /// Number of available random numbers
    nums_available: usize,
}

impl Randy {
    pub fn new() -> Randy {
        Randy {
            buf: Default::default(),
            head: 0,
            tail: 0,
            candidate: 0,
            bits_accumulated: 0,
            nums_available: 0,
        }
    }

    pub fn add_bit(&mut self, bit: usize) {
        if self.nums_available == BUF_SIZE {
            return;
        }

        self.candidate <<= 1;
        self.candidate |= bit;
        self.bits_accumulated += 1;
        if self.bits_accumulated == 3 {
            self.bits_accumulated = 0;
            // we have 7 pieces, so the indices for the Knuth shuffle
            // will be 0-6
            if self.candidate < 7 {
                self.buf[self.tail] = self.candidate;
                self.tail += 1;
                self.tail %= BUF_SIZE;
                // prepare for a new number
                self.candidate = 0;
                self.nums_available += 1;
            } else {
                // throw out the number
                self.candidate = 0;
            }
        }
    }

    pub fn nums_available(&self) -> usize {
        self.nums_available
    }
}

// NOTE: perhaps it would be better to use a PRNG seeded from the
//       ADC or temperature sensor readings, idk
impl Rng for Randy {
    fn next(&mut self) -> usize {
        if self.nums_available > 0 {
            let ret = self.buf[self.head];
            self.head += 1;
            self.head %= BUF_SIZE;
            self.nums_available -= 1;
            ret
        } else {
            // TODO: how to handle this?
            3
        }
    }
}

// -------------------------------
//        CONSTANTS
// -------------------------------
const TEXT_HEIGHT : u8 = 8;
const NUMBER_CHAR_WIDTH : u8 = 4;
const SCORE_TEXT_WIDTH : u8 = 24;
const LEVEL_TEXT_WIDTH : u8 = 25;
const BUF_SIZE : usize = 18;
const BLOCK_WIDTH : u8 = 5;
const PLAYFIELD_HORIZONTAL_PADDING : u8 = 39;
const PLAYFIELD_VERTICAL_PADDING : u8 = 9;
const PLAYFIELD_WIDTH : u8 = 10 * BLOCK_WIDTH;
const PLAYFIELD_HEIGHT : u8 = 22 * BLOCK_WIDTH;
// 26 x 8 (actually 25 x 8 - but it's easier to specify data with pixels evenly
const LEVEL_TEXT : [u8; 312] = [
    0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0x00,0xF0,0x0F,0x00,0xF0,0x0F,0xFF,0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,0xF0,0x0F,0x00,0xF0,0x0F,0x00,0xFF,0xFF,0x00,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0x00,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0x00,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0x00,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0x00,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0x00,0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,0xF0,0x0F,0x00,0xFF,0xFF,0xFF,0xFF,0xFF,0x00,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0x00,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0x00,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0x00,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0x00,0xFF,0xFF,0x00,0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0x00,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0x00,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0x00,0xFF,0xFF,0x00,0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0x00,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,
    0xF0,0x0F,0x00,0xF0,0x0F,0x00,0xFF,0xFF,0x00,0xF0,0x0F,0x00,0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,0xF0,0x0F,0x00,0xF0,0x0F,0x00,0xFF,0xFF,0x00,0xF0,0x0F,0x00,0xF0,0x0F,0xFF,
];

// 24 x 8
const SCORE_TEXT : [u8; 288] = [
    0xFF,0xFF,0x00,0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,0xF0,0x0F,0x00,0xFF,0xFF,0xFF,0xFF,0xFF,0x00,0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,0xF0,0x0F,0x00,0xFF,0xFF,0xFF,0xF0,0x0F,0x00,0xF0,0x0F,0x00,
    0xF0,0x0F,0xFF,0xFF,0xFF,0x00,0xFF,0xFF,0x00,0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,0xF0,0x0F,0xFF,0xFF,0xFF,0x00,0xFF,0xFF,0x00,0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,
    0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0x00,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,0xFF,0xFF,0x00,0xFF,0xFF,0x00,0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,
    0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0x00,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,0xFF,0xFF,0x00,0xFF,0xFF,0x00,0xFF,0xFF,0x00,0xFF,0xFF,0xFF,0xF0,0x0F,0x00,0xFF,0xFF,0xFF,
    0xFF,0xFF,0x00,0xF0,0x0F,0xFF,0xFF,0xFF,0x00,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,0xFF,0xFF,0x00,0xFF,0xFF,0x00,0xF0,0x0F,0x00,0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,
    0xFF,0xFF,0xFF,0xFF,0xFF,0x00,0xFF,0xFF,0x00,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,0xFF,0xFF,0x00,0xFF,0xFF,0x00,0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,
    0xF0,0x0F,0xFF,0xFF,0xFF,0x00,0xFF,0xFF,0x00,0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,0xF0,0x0F,0xFF,0xFF,0xFF,0x00,0xFF,0xFF,0x00,0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,
    0xFF,0xFF,0x00,0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,0xF0,0x0F,0x00,0xFF,0xFF,0xFF,0xFF,0xFF,0x00,0xF0,0x0F,0xFF,0xFF,0xFF,0x00,0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,0xF0,0x0F,0x00,0xF0,0x0F,0x00,
];

const BYTES_IN_NUM_TEXT: usize = 48;

const NUMBER_TEXT : [u8; 480] = [
    // 0
    0xFF,0xFF,0x00,0xF0,0x0F,0xFF,
    0xF0,0x0F,0xFF,0xFF,0xFF,0x00,
    0xF0,0x0F,0xFF,0xFF,0xFF,0x00,
    0xF0,0x0F,0xFF,0xFF,0xFF,0x00,
    0xF0,0x0F,0xFF,0xFF,0xFF,0x00,
    0xF0,0x0F,0xFF,0xFF,0xFF,0x00,
    0xF0,0x0F,0xFF,0xFF,0xFF,0x00,
    0xFF,0xFF,0x00,0xF0,0x0F,0xFF,
    // 1
    0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,
    0xFF,0xFF,0x00,0xF0,0x0F,0xFF,
    0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,
    0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,
    0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,
    0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,
    0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,
    0xFF,0xFF,0x00,0xF0,0x0F,0x00,
    // 2
    0xFF,0xFF,0x00,0xF0,0x0F,0xFF,
    0xF0,0x0F,0xFF,0xFF,0xFF,0x00,
    0xFF,0xFF,0xFF,0xFF,0xFF,0x00,
    0xFF,0xFF,0xFF,0xFF,0xFF,0x00,
    0xFF,0xFF,0x00,0xF0,0x0F,0xFF,
    0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,
    0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,
    0xF0,0x0F,0x00,0xF0,0x0F,0x00,
    // 3
    0xFF,0xFF,0x00,0xF0,0x0F,0xFF,
    0xF0,0x0F,0xFF,0xFF,0xFF,0x00,
    0xFF,0xFF,0xFF,0xFF,0xFF,0x00,
    0xFF,0xFF,0xFF,0xFF,0xFF,0x00,
    0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,
    0xFF,0xFF,0xFF,0xFF,0xFF,0x00,
    0xF0,0x0F,0xFF,0xFF,0xFF,0x00,
    0xFF,0xFF,0x00,0xF0,0x0F,0xFF,
    // 4
    0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,
    0xF0,0x0F,0xFF,0xF0,0x0F,0xFF,
    0xF0,0x0F,0xFF,0xF0,0x0F,0xFF,
    0xF0,0x0F,0xFF,0xF0,0x0F,0xFF,
    0xF0,0x0F,0x00,0xF0,0x0F,0x00,
    0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,
    0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,
    0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,
    // 5
    0xF0,0x0F,0x00,0xF0,0x0F,0x00,
    0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,
    0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,
    0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,
    0xFF,0xFF,0x00,0xF0,0x0F,0xFF,
    0xFF,0xFF,0xFF,0xFF,0xFF,0x00,
    0xF0,0x0F,0xFF,0xFF,0xFF,0x00,
    0xFF,0xFF,0x00,0xF0,0x0F,0xFF,
    // 6
    0xFF,0xFF,0x00,0xF0,0x0F,0xFF,
    0xF0,0x0F,0xFF,0xFF,0xFF,0x00,
    0xF0,0x0F,0xFF,0xFF,0xFF,0xFF,
    0xF0,0x0F,0x00,0xF0,0x0F,0xFF,
    0xF0,0x0F,0xFF,0xFF,0xFF,0x00,
    0xF0,0x0F,0xFF,0xFF,0xFF,0x00,
    0xF0,0x0F,0xFF,0xFF,0xFF,0x00,
    0xFF,0xFF,0x00,0xF0,0x0F,0xFF,
    // 7
    0xF0,0x0F,0x00,0xF0,0x0F,0x00,
    0xFF,0xFF,0xFF,0xFF,0xFF,0x00,
    0xFF,0xFF,0xFF,0xFF,0xFF,0x00,
    0xFF,0xFF,0xFF,0xF0,0x0F,0xFF,
    0xFF,0xFF,0x00,0xFF,0xFF,0xFF,
    0xFF,0xFF,0x00,0xFF,0xFF,0xFF,
    0xFF,0xFF,0x00,0xFF,0xFF,0xFF,
    0xFF,0xFF,0x00,0xFF,0xFF,0xFF,
    // 8
    0xFF,0xFF,0x00,0xF0,0x0F,0xFF,
    0xF0,0x0F,0xFF,0xFF,0xFF,0x00,
    0xF0,0x0F,0xFF,0xFF,0xFF,0x00,
    0xF0,0x0F,0xFF,0xFF,0xFF,0x00,
    0xFF,0xFF,0x00,0xF0,0x0F,0xFF,
    0xF0,0x0F,0xFF,0xFF,0xFF,0x00,
    0xF0,0x0F,0xFF,0xFF,0xFF,0x00,
    0xFF,0xFF,0x00,0xF0,0x0F,0xFF,
    // 9
    0xFF,0xFF,0x00,0xF0,0x0F,0xFF,
    0xF0,0x0F,0xFF,0xFF,0xFF,0x00,
    0xF0,0x0F,0xFF,0xFF,0xFF,0x00,
    0xF0,0x0F,0xFF,0xFF,0xFF,0x00,
    0xFF,0xFF,0x00,0xF0,0x0F,0x00,
    0xFF,0xFF,0xFF,0xFF,0xFF,0x00,
    0xF0,0x0F,0xFF,0xFF,0xFF,0x00,
    0xFF,0xFF,0x00,0xF0,0x0F,0xFF,
];
