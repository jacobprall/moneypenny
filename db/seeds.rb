# This file should contain all the record creation needed to seed the database with its default values.
# The data can then be loaded with the rails db:seed command (or created alongside the database with db:setup).
#
# Examples:
#
#   movies = Movie.create([{ name: 'Star Wars' }, { name: 'Lord of the Rings' }])
#   Character.create(name: 'Luke', movie: movies.first)

User.delete_all
Account.delete_all
Transaction.delete_all
Goal.delete_all
Bill.delete_all



demo = User.create(email: 'demo@email.com', password: 'password', p_num: '1234567890')

checking = Account.create(debit: true, account_category: 'Cash', institution: 'Bank of America', label: 'Checking', balance: 3652.53, user_id: demo.id)
savings = Account.create(debit: true, account_category: 'Cash', institution: 'Chase Bank', label: 'Savings', balance: 40000.00, user_id: demo.id)
invest1 = Account.create(debit: true, account_category: 'Investments', institution: 'Charles Schwab', label: 'Portfolio', balance: 14589.20, user_id: demo.id)
invest2 = Account.create(debit: true, account_category: 'Investments', institution: 'Fidelity', label: '401k', balance: 74390.78, user_id: demo.id)
loan1 = Account.create(debit: false, account_category: 'Loans', institution: 'US Bank', label: 'Mortgage', balance: 124000, user_id: demo.id)
loan2 = Account.create(debit: false, account_category: 'Loans', institution: 'Other', label: 'Student Loans', balance: 14000, user_id: demo.id)
creditcard = Account.create(debit: false, account_category: 'Credit Cards', institution: 'American Express', label: 'Amex Travel', balance: 576.90, user_id: demo.id)
property = Account.create(debit: true, account_category: 'Property', institution: 'Other', label: 'Mini Cooper', balance: 10000, user_id: demo.id)

Transaction.create(amount: -1500.00, date: Date.new(2020, 11, 3), description: 'Rent', transaction_category: 'Housing', account_id: checking.id)
Transaction.create(amount: -323.70, date: Date.new(2020, 11, 4), description: 'Lease', transaction_category: 'Transportation', account_id: checking.id)
Transaction.create(amount: -124.90, date: Date.new(2020, 11, 5), description: 'PGE', transaction_category: 'Utilities', account_id: checking.id)
Transaction.create(amount: -86.56, date: Date.new(2020, 11, 5), description: 'Ralphs', transaction_category: 'Food', account_id: checking.id)
Transaction.create(amount: -83.89, date: Date.new(2020, 11, 6), description: 'Target', transaction_category: 'Personal', account_id: checking.id)
Transaction.create(amount: -500.67, date: Date.new(2020, 11, 7), description: 'Glasses Hut', transaction_category: 'Healthcare', account_id: checking.id)
Transaction.create(amount: -19.80, date: Date.new(2020, 11, 7), description: 'AMC Movies', transaction_category: 'Recreation/Entertainment', account_id: checking.id)
Transaction.create(amount: -5.67, date: Date.new(2020, 11, 8), description: 'Starbucks', transaction_category: 'Food', account_id: checking.id)
Transaction.create(amount: 189.93, date: Date.new(2020, 11, 9), description: 'Loan Payment', transaction_category: 'Other', account_id: loan2.id)
Transaction.create(amount: -189.93, date: Date.new(2020, 11, 9), description: 'Loan Payment', transaction_category: 'Other', account_id: checking.id)
Transaction.create(amount: 73.49, date: Date.new(2020, 11, 11), description: 'Dividend', transaction_category: 'Income', account_id: invest2.id)
Transaction.create(amount: 50.00, date: Date.new(2020, 11, 11), description: 'Credit Card Payment', transaction_category: 'Miscellaneous', account_id: creditcard.id)
Transaction.create(amount: -50.00, date: Date.new(2020, 11, 11), description: 'Credit Card Payment', transaction_category: 'Miscellaneous', account_id: checking.id)
Transaction.create(amount: 3482.05, date: Date.new(2020, 11, 12), description: 'Paycheck', transaction_category: 'Income', account_id: checking.id)
Transaction.create(amount: -92.30, date: Date.new(2020, 11, 13), description: 'Auto Shop', transaction_category: 'Transportation', account_id: checking.id)
Transaction.create(amount: -43.65, date: Date.new(2020, 11, 15), description: 'Amazon', transaction_category: 'Shopping', account_id: checking.id)
Transaction.create(amount: -25.36, date: Date.new(2020, 11, 17), description: 'Ralphs', transaction_category: 'Food', account_id: checking.id)

Goal.create(goal_category: 'Retirement', title: 'Saving up for Retirement', goal_amount: 1000000, account_id: invest2.id)
Goal.create(goal_category: 'Wedding', title: 'Wedding Fund', goal_amount: 50000, account_id: savings.id)
Goal.create(goal_category: 'Travel', title: 'Thailand Vacation', goal_amount: 5000, account_id: checking.id)


Bill.create(amount: 1500, due_date: Date.new(2020, 9, 5), name: 'Rent', recurring: true, user_id: demo.id)
Bill.create(amount: 500, due_date: Date.new(2020, 9, 10), name: 'Lease', recurring: true, user_id: demo.id)





