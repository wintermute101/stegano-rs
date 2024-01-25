Embed files into image file (currentlu only PNG supported) using LSB method.
Run without parameters on png file will show how much data can be embeded.

input.png --encode --inputfile test.bin --outputfile test.png

recover file from image

test.png --decode --outputfile test.out

Uses 12 bytes as metadata to mark start of file end, and CRC.
