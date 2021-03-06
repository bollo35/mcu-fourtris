use fourtris::game_renderer::GameRenderer;
use fourtris::game_renderer::TetriminoType;

#[repr(u8)]
enum LcdCommand {
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

fn mini_delay() {
    for _ in 0..100000 {
        unsafe {
            asm!("nop");
        }
    }
}

// Pins used for the LCD
// PA4 - CS (Chip select)
// PB4 - SPI CLK
// PB7 - SSI2Tx (MOSI)
// PF0 - !RESET
// PF3 - Backlight UNUSED (only would need to be enabled if you had the jumper set appropriately)
// PF4 - D/CX (data/command)
pub struct Lcd<'a> {
    p: &'a tm4c123x::Peripherals,
}

impl Lcd<'_> {
    pub fn new(p: &tm4c123x::Peripherals) -> Lcd {
        // --------------------------------------
        // HARDWARE INITIALIZATION
        // --------------------------------------
        // port A and F setup - A4, F0, F3, F4 are needed as GPIO outputs
        // --------------------------------------
        // 1. enable clock for port A and F GPIO pins
        p.SYSCTL.rcgcgpio.modify(|r, w| unsafe { w.bits (r.bits() | 0x21) } );

        // wait for port A and F to be ready for use
        while p.SYSCTL.prgpio.read().bits() & 0x21 != 0x21 {}
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

        // --------------------------------------
        //    initialize and configure the SSI2
        // --------------------------------------
        // 1. enable ssi2 - 3rd bit enables SSI2
        p.SYSCTL.rcgcssi.modify(|r, w| unsafe { w.bits( r.bits() | 4 ) });
        // 2. enable clock for port B GPIO pins
        p.SYSCTL.rcgcgpio.modify(|r, w| unsafe { w.bits (r.bits() | 2) } );
        // wait for port B to be ready for use
        while p.SYSCTL.prgpio.read().bits() & 2 != 2 {}
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

        // wait for SSI2 to be ready
        while p.SYSCTL.prssi.read().bits() & 0b100 != 0b100 {}

        Lcd {
            p,
        }
    }

    pub fn init(&self) {
        // -------------------------------
        // CONFIGURE THE LCD FOR OPERATION
        // -------------------------------
        self.reset_high();
        self.cs_high();

        mini_delay();

        // ** SLPOUT command
        self.write_command(LcdCommand::SLPOUT);
        mini_delay();

        // ** set color mod: rgb 4-4-4
        let colmod_param = [3];
        self.write_command(LcdCommand::COLMOD);
        self.write_data(&colmod_param[..], 0);

        // ** MADCTL
        self.write_command(LcdCommand::MADCTL);
        // 0b00001000 -> set the pixel data order to RGB.
        // 0b00110000 -> set the (0,0) coord to the top-left of the LCD screen
        self.write_data(&[0xC8], 0);
    }

    pub fn display_on(&self) {
        // ** turn the display on
        self.write_command(LcdCommand::DISPON);
        // wait at least 120 ms
        mini_delay();
    }

    // -------------------------------------
    //          HIGHER ABSTRACTIONS
    // -------------------------------------
    pub fn set_drawing_area(&self, x: u8, y: u8, width: u8, height: u8) {
        // ** set column range
        self.write_command(LcdCommand::CASET);
        self.write_data(&[0, 2 + x, 0, 2 + x + width - 1], 0);

        // ** set row range
        self.write_command(LcdCommand::RASET);
        self.write_data(&[0, 3 + y, 0, 3 + y + height - 1], 0);
    }

    pub fn draw_pixels(&self, data: &[u8]) {
        self.write_command(LcdCommand::RAMWR);
        self.write_data(data, 0);
    }

    pub fn draw_pixels_repeatedly(&self, data: &[u8], repeat: usize) {
        self.write_command(LcdCommand::RAMWR);
        self.write_data(data, repeat);
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
        self.p.SSI2.sr.read().bits() & Lcd::SSI_MODULE_BUSY == Lcd::SSI_MODULE_BUSY
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
}


pub struct LcdBackend<'a> {
    lcd: Lcd<'a>,
}

impl LcdBackend<'_> {
    pub fn new(lcd: Lcd) -> LcdBackend {
        lcd.init();
        LcdBackend {
            lcd,
        }
    }

    pub fn draw_initial_screen(&mut self) {
        // 1. clear the screen to white
        self.lcd.set_drawing_area(0, 0, 128, 128);
        self.lcd.draw_pixels_repeatedly(&[0xFF, 0xFF, 0xFF], 8192);

        // 2. draw the playfield
        self.lcd.set_drawing_area(PLAYFIELD_HORIZONTAL_PADDING,
                                  PLAYFIELD_VERTICAL_PADDING,
                                  PLAYFIELD_WIDTH,
                                  PLAYFIELD_HEIGHT);
        self.lcd.draw_pixels_repeatedly(&[0x00, 0x00, 0x00], 2750);
        

        // 3. draw "LEVEL" text on the left side of the display
        self.lcd.set_drawing_area( (PLAYFIELD_HORIZONTAL_PADDING - LEVEL_TEXT_WIDTH) / 2,
                                  PLAYFIELD_VERTICAL_PADDING,
                                  LEVEL_TEXT_WIDTH + 1, // the array has an extra space, hence +1
                                  TEXT_HEIGHT);
        self.lcd.draw_pixels(&LEVEL_TEXT);
        // 4. draw level number (we always start on level 1)
        self.draw_level(1);

        // 5. draw "SCORE" text on the right side of the display
        let side_padding = (PLAYFIELD_HORIZONTAL_PADDING - SCORE_TEXT_WIDTH) / 2;
        self.lcd.set_drawing_area(PLAYFIELD_HORIZONTAL_PADDING + PLAYFIELD_WIDTH + side_padding,
                                  PLAYFIELD_VERTICAL_PADDING,
                                  SCORE_TEXT_WIDTH,
                                  TEXT_HEIGHT);
        self.lcd.draw_pixels(&SCORE_TEXT);

        // 6. no free points for you!
        self.draw_score(0);
    }

    pub fn turn_on_display(&self) {
        self.lcd.display_on();
    }

    pub fn clear_playing_field(&self) {
        // make the playing field black
        self.lcd.set_drawing_area(PLAYFIELD_HORIZONTAL_PADDING as u8,
                           PLAYFIELD_VERTICAL_PADDING as u8,
                           PLAYFIELD_WIDTH as u8,
                           PLAYFIELD_HEIGHT as u8);
        self.lcd.draw_pixels_repeatedly(&[0x00, 0x00, 0x00], 2750);
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
            };

        // set the drawing area for the tetrimino
        let xs = (x as u8)*BLOCK_WIDTH + PLAYFIELD_HORIZONTAL_PADDING;
        let ys = (y as u8)*BLOCK_WIDTH + PLAYFIELD_VERTICAL_PADDING;
        self.lcd.set_drawing_area(xs, ys, BLOCK_WIDTH, BLOCK_WIDTH);

        // write pixel data
        let repeat_count = (BLOCK_WIDTH * BLOCK_WIDTH) >> 1;
        self.lcd.draw_pixels_repeatedly(&pixel_data[..], repeat_count as usize);
    }

    fn draw_score(&mut self, score: u32) {
        // erase the old score displayed
        let side_padding = (PLAYFIELD_HORIZONTAL_PADDING - SCORE_TEXT_WIDTH) / 2;
        self.lcd.set_drawing_area(PLAYFIELD_HORIZONTAL_PADDING + PLAYFIELD_WIDTH + side_padding,
                              12 + PLAYFIELD_VERTICAL_PADDING, // 12 is arbitrary
                              SCORE_TEXT_WIDTH, // more space than we actually need to erase
                              TEXT_HEIGHT);
        self.lcd.draw_pixels_repeatedly(&[0xFF, 0xFF, 0xFF], 96);
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
            self.lcd.set_drawing_area(display_start,
                                  12 + PLAYFIELD_VERTICAL_PADDING as u8, // give some space between SCORE text and #s
                                  NUMBER_CHAR_WIDTH,  // 4 pixels wide (no trickery this time)
                                  TEXT_HEIGHT); // 8 pixels high
            self.lcd.draw_pixels(&NUMBER_TEXT[digit*BYTES_IN_NUM_TEXT..(digit+1)*BYTES_IN_NUM_TEXT]);
            display_start -= 5;
            displayed_score /= 10;
        }
    }


    fn draw_level(&mut self, level: usize) {
        // erase the old level number displayed
        self.lcd.set_drawing_area((PLAYFIELD_HORIZONTAL_PADDING - LEVEL_TEXT_WIDTH) / 2,
                                  12 + PLAYFIELD_VERTICAL_PADDING as u8,
                                  LEVEL_TEXT_WIDTH,
                                  TEXT_HEIGHT);
        self.lcd.draw_pixels_repeatedly(&[0xFF, 0xFF, 0xFF], 96);
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
            self.lcd.set_drawing_area(display_start,
                                      12 + PLAYFIELD_VERTICAL_PADDING as u8, // give some space between SCORE text and #s
                                      NUMBER_CHAR_WIDTH,  // 4 pixels wide (no trickery this time)
                                      TEXT_HEIGHT); // 8 pixels high
            self.lcd.draw_pixels(&NUMBER_TEXT[digit*BYTES_IN_NUM_TEXT..(digit+1)*BYTES_IN_NUM_TEXT]);
            display_start -= 5;
            level_copy /= 10;
        }
    }
}

// -------------------------------
//            CONSTANTS
// -------------------------------
const TEXT_HEIGHT : u8 = 8;
const NUMBER_CHAR_WIDTH : u8 = 4;
const SCORE_TEXT_WIDTH : u8 = 24;
const LEVEL_TEXT_WIDTH : u8 = 25;
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
