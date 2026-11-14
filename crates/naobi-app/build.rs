use std::error::Error;

/// A `.res` resource file holding the exe icon, written by hand so the build needs no resource
/// compiler and no extra crate: `link.exe` takes a `.res` and turns it into an object itself.
/// `ico` is an .ico file whose images are all PNG (the one in `assets/` is).
fn icon_res(ico: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
    let u16_at = |i: usize| -> Result<u16, Box<dyn Error>> {
        Ok(u16::from_le_bytes(
            ico.get(i..i + 2).ok_or("short .ico")?.try_into()?,
        ))
    };
    let u32_at = |i: usize| -> Result<u32, Box<dyn Error>> {
        Ok(u32::from_le_bytes(
            ico.get(i..i + 4).ok_or("short .ico")?.try_into()?,
        ))
    };
    // One resource: type and name are numeric ids, language 0x0409, flags MOVEABLE | PURE | DISCARDABLE.
    fn entry(out: &mut Vec<u8>, kind: u16, id: u16, data: &[u8]) {
        out.extend((data.len() as u32).to_le_bytes());
        out.extend(32u32.to_le_bytes());
        for v in [0xFFFFu16, kind, 0xFFFF, id] {
            out.extend(v.to_le_bytes());
        }
        out.extend(0u32.to_le_bytes());
        out.extend(0x1030u16.to_le_bytes());
        out.extend(0x0409u16.to_le_bytes());
        out.extend([0u8; 8]);
        out.extend(data);
        out.resize(out.len().next_multiple_of(4), 0);
    }

    let count = usize::from(u16_at(4)?);
    let mut res = Vec::new();
    // The mandatory empty first resource.
    res.extend([0u8; 4]);
    res.extend(32u32.to_le_bytes());
    res.extend([0xFF, 0xFF, 0, 0, 0xFF, 0xFF, 0, 0]);
    res.extend([0u8; 16]);
    let mut group = [0u16, 1, count as u16]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect::<Vec<u8>>();
    for i in 0..count {
        let e = 6 + 16 * i;
        let (size, at) = (u32_at(e + 8)? as usize, u32_at(e + 12)? as usize);
        let image = ico.get(at..at + size).ok_or("short .ico image")?;
        entry(&mut res, 3, i as u16 + 1, image); // RT_ICON
                                                 // GRPICONDIRENTRY: the 12 header bytes of the .ico entry, then the resource id.
        group.extend(ico.get(e..e + 12).ok_or("short .ico")?);
        group.extend((i as u16 + 1).to_le_bytes());
    }
    entry(&mut res, 14, 1, &group); // RT_GROUP_ICON, the lowest id is the exe's icon
    Ok(res)
}

fn main() -> Result<(), Box<dyn Error>> {
    let dir = env!("CARGO_MANIFEST_DIR");
    let manifest = format!("{dir}/app.manifest");
    println!("cargo:rerun-if-changed={manifest}");
    // asInvoker, per-monitor-v2 DPI and long paths, embedded without an extra crate.
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{manifest}");

    let ico = format!("{dir}/assets/naobi.ico");
    println!("cargo:rerun-if-changed={ico}");
    let res = std::path::Path::new(&std::env::var("OUT_DIR")?).join("icon.res");
    std::fs::write(&res, icon_res(&std::fs::read(&ico)?)?)?;
    println!("cargo:rustc-link-arg-bins={}", res.display());
    Ok(())
}
