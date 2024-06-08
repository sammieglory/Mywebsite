// SPDX-License-Identifier: Unlicense

use std::path::PathBuf;
use std::convert::TryInto; // Import TryInto for the conversion

use crate::luhn::AccountNumber;

use rand::prelude::*;
use rusqlite::{Connection, Result};


#[derive(Debug)]
pub struct Account {
    pub id: u64,
    pub account_number: String,
    pub balance: usize,
    pub pin: String,
}

impl Account {
    pub fn new() -> Result<Self> {
        let mut new_account_number = AccountNumber::default();
        let balance = 0;

        let db = initialise_bankdb()?;
        loop {
            let query_string = format!(
                "SELECT 1 FROM account where account_number='{}';",
                new_account_number
            );

            match db.query_row(&query_string, [], |row| row.get::<usize, usize>(0)) {
                Ok(_) => {
                    new_account_number = AccountNumber::default();
                }
                Err(_) => break,
            }
        }

        let account = create_account(&new_account_number, balance)?;

        Ok(account)
    }
}

#[cfg(not(test))]
fn database_path() -> PathBuf {
    PathBuf::from("bank.s3db")
}

#[cfg(test)]
fn database_path() -> PathBuf {
    PathBuf::from("mock_bank.s3db")
}

pub fn initialise_bankdb() -> Result<Connection> {
    let db = Connection::open(database_path())?;

    let command = "CREATE TABLE IF NOT EXISTS account(
id INTEGER PRIMARY KEY,
account_number TEXT,
pin TEXT DEFAULT '000000',
balance INTEGER DEFAULT 0
)
";

    db.execute(command, ())?;
    Ok(db)
}

pub fn create_account(data: &AccountNumber, balance: u64) -> Result<Account> {
    let db = initialise_bankdb()?;
    let account_number = data.to_string();

    let newest_max_id = db.query_row(
        "SELECT COALESCE(MAX(id), 0) + 1 FROM account",
        [],
        |row| row.get(0),
    )?;

    let mut rng = thread_rng();
    let mut pin: Vec<String> = Vec::new();

    // Six digit pin
    for _ in 1..=6 {
        let y = rng.gen_range(0..=9).to_string();
        pin.push(y);
    }

    let pin: String = pin.into_iter().collect();

    let new_account = Account {
        id: newest_max_id,
        account_number,
        balance: balance.try_into().unwrap(), // Convert u64 to usize
        pin,
    };

    db.execute(
        "INSERT INTO account (id, account_number, pin, balance) VALUES (?1, ?2, ?3, ?4)",
        (
            &new_account.id,
            &new_account.account_number,
            &new_account.pin,
            &new_account.balance,
        ),
    )?;
    Ok(new_account)
}

pub fn deposit(amount: &str, pin: &str, account_number: &str) -> Result<()> {
    let db = initialise_bankdb()?;
    let query_string = format!(
        "SELECT pin FROM account where account_number='{}';",
        account_number
    );

    let pin_from_db: String = db.query_row(&query_string, [], |row| row.get(0))?;

    let correct_pin = { pin_from_db == pin };

    if correct_pin {
        db.execute(
            "UPDATE account SET balance = balance + ?1 WHERE account_number=?2",
            (amount, account_number),
        )?;

        let query_string = format!(
            "SELECT balance FROM account where account_number='{}';",
            account_number
        );

        let amount_from_db: usize = db.query_row(&query_string, [], |row| row.get(0))?;

        println!(
            "The account number `{}` now has a balance of `{}`.\n",
            &account_number, &amount_from_db
        );
    } else {
        eprintln!("Wrong pin. Try again...");
    }
    Ok(())
}

pub fn transfer(
    amount: &str,
    pin: &str,
    origin_account: &str,
    target_account: &str,
) -> Result<(Account, Account)> {
    if *origin_account == *target_account {
        return Err(rusqlite::Error::QueryReturnedNoRows); // Makes sense. We haven't returned any.
    }

    let origin_account = fetch_account(origin_account)?;
    let target_account = fetch_account(target_account)?;

    let correct_pin = origin_account.pin == pin;

    if correct_pin {
        let amount = amount
            .parse::<u64>().map_err(|_| {
                rusqlite::Error::QueryReturnedNoRows
            })?;

        if amount > origin_account.balance as u64 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        } else {
            let db = initialise_bankdb()?;
            db.execute(
                "UPDATE account SET balance = balance + ?1 WHERE account_number=?2",
                (amount, &target_account.account_number),
            )?;

            db.execute(
                "UPDATE account SET balance = balance - ?1 WHERE account_number=?2",
                (amount, &origin_account.account_number),
            )?;
        };
    } else {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }

    let origin_account = fetch_account(&origin_account.account_number)?;
    let target_account = fetch_account(&target_account.account_number)?;

    Ok((origin_account, target_account))
}

pub fn withdraw(amount: &str, pin: &str, account_number: &str) -> Result<()> {
    let db = initialise_bankdb()?;
    let query_string = format!(
        "SELECT pin FROM account where account_number='{}';",
        account_number
    );

    let pin_from_db: String = db.query_row(&query_string, [], |row| row.get(0))?;

    let correct_pin = { pin_from_db == pin };

    if correct_pin {
        let query_string = format!(
            "SELECT balance FROM account where account_number='{}';",
            account_number
        );

        let amount_from_db: usize = db.query_row(&query_string, [], |row| row.get(0))?;

        println!(
            "The account number `{}` has a balance of `{}`.\n",
            &account_number, &amount_from_db
        );

        let amount = amount
            .parse::<usize>()
            .expect("Not able to parse string to usize");

        if amount > amount_from_db {
            eprintln!(
                "You are trying to withdraw an amount that exceeds your current deposit... aborting...\n"
            );
        } else {
            db.execute(
                "UPDATE account SET balance = balance - ?1 WHERE account_number=?2",
                (amount, account_number),
            )?;

            let query_string = format!(
                "SELECT balance FROM account where account_number='{}';",
                account_number
            );

            let amount_from_db: usize = db.query_row(&query_string, [], |row| row.get(0))?;

            println!(
                "The account number `{}` now has a balance of `{}`.\n",
                &account_number, &amount_from_db
            );
        };
    } else {
        eprintln!("Wrong pin. Try again...");
    }
    Ok(())
}

pub fn delete_account(account_number: &str, pin: &str) -> Result<()> {
    let db = initialise_bankdb()?;
    let query_string = format!(
        "SELECT pin FROM account where account_number='{}';",
        &account_number
    );

    let pin_from_db: String = db.query_row(&query_string, [], |row| row.get(0))?;
    let correct_pin = { pin_from_db == pin };

    if correct_pin {
        db.execute(
            "DELETE FROM account WHERE account_number=?1",
            (account_number,),
        )?;
        println!("DELETED ACCOUNT: {}", &account_number);
    } else {
        eprintln!("Wrong pin. Try again...");
    }
    Ok(())
}

pub fn show_balance(account_number: &str) -> Result<()> {
    let db = initialise_bankdb()?;
    let query_string = format!(
        "SELECT balance FROM account where account_number='{}';",
        account_number
    );

    let amount_from_db: usize = db.query_row(&query_string, [], |row| row.get(0))?;

    println!(
        "The account number `{}` now has a balance of `{}`.\n",
        &account_number, &amount_from_db
    );
    Ok(())
}

fn fetch_account(account: &str) -> Result<Account> {
    let db = initialise_bankdb()?;
    let mut stmt = db.prepare("SELECT id, account_number, balance, pin FROM account")?;
    let accounts = stmt.query_map([], |row| {
        Ok(Account {
            id: row.get(0)?,
            account_number: row.get(1)?,
            balance: row.get(2)?,
            pin: row.get(3)?,
        })
    })?;

    let accounts = accounts.flatten().find(|acc| acc.account_number == account);
    if let Some(fetched_account) = accounts {
        Ok(fetched_account)
    } else {
        Err(rusqlite::Error::QueryReturnedNoRows)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn created_account_is_correct_fetched_from_db() -> Result<()> {
        let acc1 = Account::new()?;
        let acc2 = fetch_account(&acc1.account_number)?;

        assert_eq!(acc1.id, acc2.id);

        Ok(())
    }

    #[test]
    fn transferred_balance_is_correct() -> Result<()> {
        // Step 1: Create two new accounts
        let origin_account = Account::new()?;
        let target_account = Account::new()?;
        let deposit_balance = "10000";

        // Deposit into the origin account
        deposit(deposit_balance, &origin_account.pin, &origin_account.account_number)?;

        // Fetch the updated origin account to get the new balance
        let origin_account = fetch_account(&origin_account.account_number)?;
        assert_eq!(*deposit_balance, origin_account.balance.to_string());

        // Step 2: Transfer the entire balance from origin account to target account
        transfer(deposit_balance, &origin_account.pin, &origin_account.account_number, &target_account.account_number)?;

        // Fetch updated account balances after transfer
        let origin_account = fetch_account(&origin_account.account_number)?;
        let target_account = fetch_account(&target_account.account_number)?;

        // Assertions to verify the balances
        assert_eq!("0".to_string(), origin_account.balance.to_string());
        assert_eq!(deposit_balance.to_owned(), target_account.balance.to_string());

        Ok(())
    }
}

