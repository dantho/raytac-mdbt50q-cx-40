/* Raytac MDBT50Q-CX-40 -> Nordic nRF52840 (1 MB flash, 256 kB RAM).

   Layout confirmed by querying the bootloader (DFU FIRMWARE_VERSION, 0x0B):
     0x00000000  MBR            (4 kB, reserved)
     0x00001000  application    <- this image
     0x000F4000  bootloader     (0xA000 long)
  No SoftDevice is installed. The board has an external 32.768 kHz crystal.
  The MBR reserves the first 8 bytes of RAM. */
MEMORY
{
  FLASH : ORIGIN = 0x00001000, LENGTH = 0xF3000
  RAM   : ORIGIN = 0x20000008, LENGTH = 256K - 8
}
