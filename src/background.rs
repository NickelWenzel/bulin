use tokio::time::{sleep, Duration};
use anyhow::Result;

pub struct BackgroundTask {
    pub counter: u32,
    pub is_running: bool,
}

impl BackgroundTask {
    pub fn new() -> Self {
        Self {
            counter: 0,
            is_running: false,
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        self.is_running = true;
        
        while self.is_running {
            // Simulate some background work
            sleep(Duration::from_secs(1)).await;
            self.counter += 1;
            
            // Log progress
            if self.counter % 10 == 0 {
                println!("Background task counter: {}", self.counter);
            }
        }
        
        Ok(())
    }

    pub fn stop(&mut self) {
        self.is_running = false;
    }

    pub fn reset(&mut self) {
        self.counter = 0;
    }
}

pub async fn fetch_data() -> Result<String> {
    // Simulate async data fetching
    sleep(Duration::from_millis(500)).await;
    Ok("Fetched data from async operation".to_string())
}

pub async fn process_data(data: String) -> Result<String> {
    // Simulate async data processing
    sleep(Duration::from_millis(200)).await;
    Ok(format!("Processed: {}", data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fetch_data() {
        let result = fetch_data().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Fetched data from async operation");
    }

    #[tokio::test]
    async fn test_process_data() {
        let input = "test data".to_string();
        let result = process_data(input).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Processed: test data");
    }
}
