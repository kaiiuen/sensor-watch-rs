/* Memory layout for the Microchip SAM L22J18A (Sensor-Watch)
 *
 * 0x00000000-0x00002000: Bootloader       (0x2000 / 8192 bytes)
 * 0x00002000-0x0003C000: Firmware         (0x3A000 / 237568 bytes)
 * 0x0003C000-0x00040000: EEPROM Emulation (0x2000 / 8192 bytes)
 * 0x20000000-0x20008000: RAM              (0x8000 / 32768 bytes)
 */
MEMORY
{
  FLASH : ORIGIN = 0x00002000, LENGTH = 0x3A000
  RAM   : ORIGIN = 0x20000000, LENGTH = 0x8000
}

/* RAM-resident code (`.ramfunc`): flash-write routines must execute from RAM.
 * If the CPU runs a function in flash bank A while writing to bank B (the RWW
 * EEPROM area), the memory bus stalls. Placing these routines in RAM keeps the
 * CPU running during the write. The startup code copies them from FLASH to RAM
 * before main runs.
 *
 * `INSERT AFTER .data` places the `.ramfunc` LMA (load address in flash) after
 * the main code sections (`.text`, `.rodata`, `.data`), so it does not overlap
 * the vector table at the flash origin. */
SECTIONS
{
  .ramfunc : ALIGN(4)
  {
    __ramfunc_start = .;
    *(.ramfunc .ramfunc.*);
    __ramfunc_end = .;
  } > RAM AT> FLASH

  /* LMA (load address) of the .ramfunc section, for the startup copy. */
  __sramfunc_lma = LOADADDR(.ramfunc);
} INSERT AFTER .data;
