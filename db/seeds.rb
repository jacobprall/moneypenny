# This file should contain all the record creation needed to seed the database with its default values.
# The data can then be loaded with the rails db:seed command (or created alongside the database with db:setup).
#
# Examples:
#
#   movies = Movie.create([{ name: 'Star Wars' }, { name: 'Lord of the Rings' }])
#   Character.create(name: 'Luke', movie: movies.first)

User.delete_all
Account.delete_all

User.create(email: 'demo@email.com', password: 'password', p_num: '1234567890')

Account.create(debit: true, account_category: 'Cash', institution: 'Bank of America', label: 'Checking', balance: 3652.53, user_id: 1)
Account.create(debit: true, account_category: 'Cash', institution: 'Chase Bank', label: 'Savings', balance: 4000.00, user_id: 1)
Account.create(debit: true, account_category: 'Investments', institution: 'Charles Schwab', label: 'Portfolio', balance: 14589.20, user_id: 1)
Account.create(debit: true, account_category: 'Investments', institution: 'Fidelity', label: '401k', balance: 74390.78, user_id: 1)
Account.create(debit: false, account_category: 'Loans', institution: 'US Bank', label: 'Mortgage', balance: 124000, user_id: 1)
Account.create(debit: false, account_category: 'Loans', institution: 'Other', label: 'Student Loans', balance: 14000, user_id: 1)
Account.create(debit: false, account_category: 'Credit Cards', institution: 'American Express', label: 'Amex Travel', balance: 576.90, user_id: 1)
Account.create(debit: true, account_category: 'Property', institution: 'Other', label: 'Mini Cooper', balance: 10000, user_id: 1)
