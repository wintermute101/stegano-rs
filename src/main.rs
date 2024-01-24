use std::fs::File;
use std::io::{BufWriter, BufReader};
use std::io::prelude::*;
use std::path::Path;
use png::{Decoder, Encoder, Reader, Info};
use clap::Parser;
use std::collections::VecDeque;
use crc::{Crc, CRC_32_CKSUM};
static FILEBEG: u32 = 0x0f1f2fff;
static FILEEND: u32 = 0x0e1e2eee;

struct ImageInfo{
    widht : u32,
    height : u32,
    channels : u8,
    bit_depth : u8,
    maxencode: u64,
}

impl From<&Info<'_>> for ImageInfo{
    fn from(item: &Info) -> Self{
        use png::ColorType::*;
        let channels : u8 = match item.color_type{ 
            Grayscale | Indexed => 1,
            Rgb => 3,
            GrayscaleAlpha => 2,
            Rgba => 4,
        };
        let max_encoded_data = (item.width as u64)*(item.height as u64)*(channels as u64)/8;
        ImageInfo{ widht: item.width, height: item.height, channels: channels, bit_depth: item.bit_depth as u8, maxencode: max_encoded_data}   
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Name of the person to greet
    #[arg(required = true)]
    filename: String,

    #[arg(long)]
    outputfile: Option<String>,

    #[arg(long)]
    inputfile: Option<String>,

    #[arg(short, long, requires = "inputfile", requires = "outputfile")]
    encode: bool,

    #[arg(short, long, requires = "outputfile")]
    decode: bool
}

fn main() {
    let args: Args = Args::parse();

    let decoder = Decoder::new(File::open(args.filename).unwrap());
    let mut reader = decoder.read_info().unwrap();

    let image_info = check_info(&reader);

    let mut buf = vec![0; reader.output_buffer_size()];

    if args.decode{
        println!("Reading file...");
        let _r = reader.next_frame(&mut buf).unwrap();
        decode(&buf, image_info, &args.outputfile.unwrap());
    } else if args.encode{
        println!("Reading file...");
        let _r = reader.next_frame(&mut buf).unwrap();
        encode(&reader, &mut buf, image_info, &args.outputfile.unwrap(), &args.inputfile.unwrap());
    }

}

enum SearchState{
    FoundStart(u8),
    FoundEnd(u8),
    CRC(u8),
}

fn decode(buf: &Vec<u8>, image_info: ImageInfo, outputfile: &str){
    let len = (image_info.height*image_info.widht*(image_info.channels as u32)*((image_info.bit_depth/8) as u32)) as usize;

    let mut counter = 0;
    let mut byte: u8 = 0;

    let path = Path::new(outputfile);
    let file = File::create(path).unwrap();
    let ref mut writer = BufWriter::new(file);

    let start = FILEBEG.to_be_bytes();
    let end = FILEEND.to_be_bytes();

    let mut state = SearchState::FoundStart(0);

    let mut bytecounter = 0;

    let crc = Crc::<u32>::new(&CRC_32_CKSUM);
    let mut digest = crc.digest();
    let mut readcrc = 0u32;

    for i in (0usize..len).step_by((image_info.bit_depth/8) as usize){
        let subpix: u8 = if image_info.bit_depth == 8 {
                buf[i].into()
            }
            else if image_info.bit_depth == 16{
                buf[i+1].into()
            }
            else{
                0
            };

            let bit = subpix & 0x01;
            byte = byte << 1 | bit;
            //print!("{}", bit & 0x01);

            counter += 1;

            if counter == 8{

                match state{
                    SearchState::FoundStart(n) if n < 3 => {
                        if byte == start[n as usize]{
                            state = SearchState::FoundStart(n+1);
                            //println!("Found start {}", n);
                        }
                        else{
                            state = SearchState::FoundStart(0);
                        }
                    },
                    SearchState::FoundStart(n) => {
                        if byte == start[n as usize]{
                            //println!("Found start {}, searchind end marker", n);
                            state = SearchState::FoundEnd(0);
                        }
                        else{
                            state = SearchState::FoundStart(0);
                        }
                    }
                    SearchState::FoundEnd(n) if n < 3 =>{
                        if byte == end[n as usize]{
                            state = SearchState::FoundEnd(n+1);
                            //println!("Found end {} at {}", n, bytecounter);
                        }
                        else{
                            state = SearchState::FoundEnd(0);
                            if n > 0{
                                writer.write(&end[0..n as usize]).unwrap();
                                digest.update(&end[0..n as usize]);
                            }
                            writer.write(&[byte]).unwrap();
                            digest.update(&[byte]);
                        }
                    }
                    SearchState::FoundEnd(n) =>{
                        if byte == end[n as usize]{
                            //println!("Found end marker! {}", n);
                            state = SearchState::CRC(0);
                            bytecounter += 1;
                            //break;
                        }else{
                            if n > 0{
                                writer.write(&end[0..n as usize]).unwrap();
                                digest.update(&end[0..n as usize]);
                            }
                            writer.write(&[byte]).unwrap();
                            digest.update(&[byte]);
                        }
                    }
                    SearchState::CRC(n) =>{
                        state = SearchState::CRC(n+1);
                        readcrc = readcrc << 8 | byte as u32;
                        if n == 3{
                            state = SearchState::CRC(4);
                            bytecounter -= 4; //correct for CRC
                            break;
                        }
                    }
                }

                bytecounter += 1;

                //println!(" read {:02x}", byte);
                counter = 0;
            }
    };

    match state {
        SearchState::FoundStart(_) => {
            println!("Error did not find start marker");
        }
        SearchState::FoundEnd(n) =>{
            if n != 4{
                println!("Error did not find end marker file is probably corrupt");
            }
        }
        SearchState::CRC(n) =>{
            if n == 4{
                let filecrc = digest.finalize();
                print!("File {} of length {} successfully recovered", outputfile, bytecounter - 8);
                if filecrc == readcrc{
                    println!(" CRC OK!");
                }
                else{
                    println!(" CRC Error!");
                }
                println!("File CRC 0x{:08x} stored CRC 0x{:08x}", filecrc, readcrc);
            }
            else{
                println!("Error could not read CRC");
            }
        }
    }
}

fn encode(reader : &Reader<File>, buf: &mut Vec<u8>, image_info: ImageInfo, outputfile: &str, inputfile: &str) {

    let path = Path::new(outputfile);
    let file = File::create(path).unwrap();
    let ref mut writer = BufWriter::new(file);

    let mut encbytes: VecDeque<u8> = VecDeque::new();

    encbytes.extend(FILEBEG.to_be_bytes());

    {
        let path = Path::new(inputfile);
        let file = File::open(path).unwrap();
        let meta = file.metadata().unwrap();
        let inputlen = meta.len();
        let mut inputreader = BufReader::new(file);

        if (inputlen + 12)> image_info.maxencode{
            panic!("File too large to encode in image file {} max bytes {}", inputlen + 8, image_info.maxencode);
        }

        let crc = Crc::<u32>::new(&CRC_32_CKSUM);
        let mut digest = crc.digest();

        let mut buf = [0u8; 1024];
        loop{
            let n = inputreader.read(&mut buf).unwrap();

            if n == 0{
                break;
            }
            digest.update(&buf[0..n]);
            encbytes.extend(buf[0..n].iter());
        }

        encbytes.extend(FILEEND.to_be_bytes());

        let filecrc = digest.finalize();
        encbytes.extend(filecrc.to_be_bytes());
        println!("File CRC32 0x{:08x}", filecrc);
    }

    let mut enc = Encoder::with_info(writer, reader.info().clone()).unwrap();

    enc.set_compression(png::Compression::Default);

    let len = (image_info.height*image_info.widht*(image_info.channels as u32)*((image_info.bit_depth/8) as u32)) as usize;

    let mut byte = 0;
    let mut count = 8;

    if let Some(n) = encbytes.pop_front(){
        byte = n;
        //println!("puting {:02x}", byte);
    }

    let mut bytecounter = 1usize;

    for i in (0usize..len).step_by((image_info.bit_depth/8) as usize){
        let mut subpix: u8 = if image_info.bit_depth == 8 {
                buf[i].into()
            }
            else if image_info.bit_depth == 16{
                buf[i+1].into()
            }
            else{
                0
            };

            //print!("{}", (byte & 0x80) >> 7);

            subpix = (subpix & 0xFE) | (byte & 0x80) >> 7;
            count -= 1;

            if image_info.bit_depth == 8 {
                buf[i] = subpix;
            }
            else if image_info.bit_depth == 16{
                buf[i+1] = subpix;
            }

            if count == 0{
                if let Some(n) = encbytes.pop_front(){
                    byte = n;
                    //println!(" puting {:02x}", byte);
                    count = 8;
                    bytecounter += 1;
                }
                else{
                    break;
                }
            }
            else{
                byte = byte << 1;
            }
    };

    let mut writer = enc.write_header().unwrap();

    println!("Encoded {} bytes with {} metadata bytes. Wrinting output file...", bytecounter, 8+4);
    writer.write_image_data(&buf).unwrap();

    writer.finish().unwrap();
}

fn check_info(reader : &Reader<File>) -> ImageInfo {
    let info = reader.info();

    if info.palette.is_some(){
        println!("Image with pallette not supported!");
        todo!();
    }

    if info.interlaced{
        println!("Interlaced Image not supported!");
        todo!();
    }

    match info.bit_depth{
        png::BitDepth::One => {todo!()},
        png::BitDepth::Two => {todo!()},
        png::BitDepth::Four => {todo!()},
        png::BitDepth::Eight => {},
        png::BitDepth::Sixteen => {},
    }

    let possible_bit_per_pixel = match info.color_type {
        png::ColorType::Rgb => {3},
        png::ColorType::Rgba => {4},
        png::ColorType::Grayscale => {1},
        png::ColorType::Indexed => {todo!()},
        png::ColorType::GrayscaleAlpha => {2},
    };

    let ret = ImageInfo::from(info);

    println!("Bytes per pixel {} bits per pixel {} channels {}", info.bytes_per_pixel(), info.bits_per_pixel(), possible_bit_per_pixel);
    println!("Image size {} widh {} height buffer size {}", info.width, info.height, reader.output_buffer_size());
    println!("Max encoded data {} B {:.1} KiB", ret.maxencode, ret.maxencode as f32/1024.0f32);

    ret
}
