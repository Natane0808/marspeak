use marspeak::{Level, Marspeak};

fn main() {
    let text = "今天天气真好，我们一起去看看外面的世界，时间过得真快";

    let presets = [
        ("轻度", Level::Light, 0.4f32),
        ("中度", Level::Medium, 0.7f32),
        ("重度", Level::Heavy, 1.0f32),
    ];

    for (label, level, intensity) in presets {
        let mp = Marspeak::builder()
            .level(level)
            .intensity(intensity)
            .seed(42)
            .build()
            .expect("参数合法");

        let encoded = mp.encode(text);
        let decoded = mp.decode(&encoded).expect("字形替换是可逆的");

        println!("{label}（intensity {intensity}）");
        println!("  火星文：{encoded}");
        println!("  还原后：{decoded}");
        assert_eq!(decoded, text);
    }
}
