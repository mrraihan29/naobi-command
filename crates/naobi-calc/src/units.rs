//! Static unit tables. `f` converts to the category base unit; temperature is affine.

pub struct Unit {
    pub cat: u8,
    names: &'static [&'static str],
    f: f64,
}

const TEMP: u8 = 2;

const fn u(cat: u8, names: &'static [&'static str], f: f64) -> Unit {
    Unit { cat, names, f }
}

#[rustfmt::skip]
static UNITS: &[Unit] = &[
    // 0 length, base m
    u(0, &["mm", "millimeter", "millimeters", "millimetre", "millimetres"], 0.001),
    u(0, &["cm", "centimeter", "centimeters", "centimetre", "centimetres"], 0.01),
    u(0, &["m", "meter", "meters", "metre", "metres"], 1.0),
    u(0, &["km", "kilometer", "kilometers", "kilometre", "kilometres"], 1000.0),
    u(0, &["in", "inch", "inches"], 0.0254),
    u(0, &["ft", "foot", "feet"], 0.3048),
    u(0, &["yd", "yard", "yards"], 0.9144),
    u(0, &["mi", "mile", "miles"], 1609.344),
    // 1 mass, base g
    u(1, &["mg", "milligram", "milligrams"], 0.001),
    u(1, &["g", "gram", "grams"], 1.0),
    u(1, &["kg", "kilogram", "kilograms"], 1000.0),
    u(1, &["lb", "lbs", "pound", "pounds"], 453.592_37),
    u(1, &["oz", "ounce", "ounces"], 28.349_523_125),
    // 2 temperature, base C (names[0] picks the affine pair)
    u(TEMP, &["c", "celsius"], 1.0),
    u(TEMP, &["f", "fahrenheit"], 1.0),
    u(TEMP, &["k", "kelvin"], 1.0),
    // 3 volume (US), base l
    u(3, &["ml", "milliliter", "milliliters", "millilitre", "millilitres"], 0.001),
    u(3, &["l", "liter", "liters", "litre", "litres"], 1.0),
    u(3, &["tsp", "teaspoon", "teaspoons"], 0.004_928_921_593_75),
    u(3, &["tbsp", "tablespoon", "tablespoons"], 0.014_786_764_781_25),
    u(3, &["cup", "cups"], 0.236_588_236_5),
    u(3, &["pt", "pint", "pints"], 0.473_176_473),
    u(3, &["qt", "quart", "quarts"], 0.946_352_946),
    u(3, &["gal", "gallon", "gallons"], 3.785_411_784),
    // 4 data size, base B; "b" bit and "B" byte are told apart by exact-case match first
    u(4, &["b", "bit", "bits"], 0.125),
    u(4, &["B", "byte", "bytes"], 1.0),
    u(4, &["KB"], 1e3),
    u(4, &["MB"], 1e6),
    u(4, &["GB"], 1e9),
    u(4, &["TB"], 1e12),
    u(4, &["KiB"], 1024.0),
    u(4, &["MiB"], 1_048_576.0),
    u(4, &["GiB"], 1_073_741_824.0),
    u(4, &["TiB"], 1_099_511_627_776.0),
    // 5 time, base s
    u(5, &["ms", "millisecond", "milliseconds"], 0.001),
    u(5, &["s", "sec", "second", "seconds"], 1.0),
    u(5, &["min", "minute", "minutes"], 60.0),
    u(5, &["h", "hr", "hour", "hours"], 3600.0),
    u(5, &["d", "day", "days"], 86_400.0),
    u(5, &["wk", "week", "weeks"], 604_800.0),
    // 6 speed, base m/s
    u(6, &["m/s"], 1.0),
    u(6, &["km/h", "kph"], 1.0 / 3.6),
    u(6, &["mph"], 0.447_04),
    u(6, &["kn", "knot", "knots"], 1852.0 / 3600.0),
    // 7 area, base m2
    u(7, &["mm2"], 1e-6),
    u(7, &["cm2"], 1e-4),
    u(7, &["m2"], 1.0),
    u(7, &["km2"], 1e6),
    u(7, &["ha", "hectare", "hectares"], 1e4),
    u(7, &["ft2"], 0.092_903_04),
    u(7, &["in2"], 0.000_645_16),
    u(7, &["ac", "acre", "acres"], 4_046.856_422_4),
];

/// Exact-case match first (so `b` and `B` differ), then case-insensitive.
pub fn lookup(name: &str) -> Option<&'static Unit> {
    UNITS.iter().find(|u| u.names.contains(&name)).or_else(|| {
        UNITS
            .iter()
            .find(|u| u.names.iter().any(|n| n.eq_ignore_ascii_case(name)))
    })
}

pub fn convert(v: f64, from: &Unit, to: &Unit) -> f64 {
    if from.cat != TEMP {
        return v * from.f / to.f;
    }
    let c = match from.names[0] {
        "f" => (v - 32.0) * 5.0 / 9.0,
        "k" => v - 273.15,
        _ => v,
    };
    match to.names[0] {
        "f" => c * 9.0 / 5.0 + 32.0,
        "k" => c + 273.15,
        _ => c,
    }
}
