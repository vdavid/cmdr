#[cfg(test)]
mod test {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_calculate_text_width() {
        // Create test metrics
        let mut widths = HashMap::new();
        widths.insert('b' as u32, 7.37);
        widths.insert('e' as u32, 6.86);
        widths.insert('n' as u32, 7.00);
        widths.insert('c' as u32, 6.71);
        widths.insert('h' as u32, 7.06);
        widths.insert('_' as u32, 7.00);
        widths.insert('a' as u32, 6.62);
        widths.insert('f' as u32, 4.34);
        widths.insert('t' as u32, 4.36);
        widths.insert('r' as u32, 4.57);
        widths.insert('1' as u32, 5.57);
        widths.insert('.' as u32, 3.56);
        widths.insert('p' as u32, 7.32);
        widths.insert('g' as u32, 7.31);

        let metrics = FontMetrics::new("test".to_string(), widths);
        
        let width = metrics.calculate_text_width("bench_after_1.png");
        
        // Expected from Python: 106.52px
        assert!((width - 106.52).abs() < 0.1, "Expected ~106.52px, got {}", width);
    }
}
