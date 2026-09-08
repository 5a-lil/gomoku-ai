fn static_evaluation_diagright(board: &Vec<Cell>, size: usize) -> i64 {
        let mut final_score = 0;
        let mut count = 0;
        let mut current_state = State::Empty;
        for i in 0..size-1 {
            let mut i: i64 = i as i64;
            loop {
                let elem = board[i as usize];
                if elem.state != current_state {
                    final_score += Patterns::from(count).value(current_state);
                    current_state = elem.state;
                    count = 0;
                }
                count+=1;
                i = i + size as i64 - 1;
                if (i + 1) % size as i64 == 0 || i >= size as i64 * size as i64 {
                    break;
                }
            }
            final_score += Patterns::from(count).value(current_state);
            current_state = State::Empty;
            count = 0;
        }

        for i in 0..size {
            let mut i: i64 = i as i64 + (size as i64 * (size as i64 - 1));
            loop {
                println!("{i}");
                let elem = board[i as usize];
                if elem.state != current_state {
                    final_score += Patterns::from(count).value(current_state);
                    current_state = elem.state;
                    count = 0;
                }
                count+=1;
                i = i - size as i64 + 1;
                if i % size as i64 == 0 || i < 0 as i64 {
                    println!("break");
                    break;
                }
            }
            final_score += Patterns::from(count).value(current_state);
            current_state = State::Empty;
            count = 0;
        }
        
        final_score
    }

    fn static_evaluation_diagleft(board: &Vec<Cell>, size: usize) -> i64 {
        let mut final_score = 0;
        let mut count = 0;
        let mut current_state = State::Empty;
        for i in 1..size {
            let mut i: i64 = i as i64;
            loop {
                println!("{i}");
                let elem = board[i as usize];
                if elem.state != current_state {
                    final_score += Patterns::from(count).value(current_state);
                    current_state = elem.state;
                    count = 0;
                }
                count+=1;
                i = i + size as i64 + 1;
                if i % size as i64 == 0 || i >= size as i64 * size as i64 {
                    println!("break");
                    break;
                }
            }
            final_score += Patterns::from(count).value(current_state);
            current_state = State::Empty;
            count = 0;
        }

        for i in 0..size {
            let mut i: i64 = i as i64 + (size as i64 * (size as i64 - 1));
            loop {
                println!("{i}");
                let elem = board[i as usize];
                if elem.state != current_state {
                    final_score += Patterns::from(count).value(current_state);
                    current_state = elem.state;
                    count = 0;
                }
                count+=1;
                i = i - size as i64 - 1;
                if (i + 1) % size as i64 == 0 || i < 0 as i64 {
                    println!("break");
                    break;
                }
            }
            final_score += Patterns::from(count).value(current_state);
            current_state = State::Empty;
            count = 0;
        }
        
        final_score
    }