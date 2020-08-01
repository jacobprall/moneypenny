# This file should contain all the record creation needed to seed the database with its default values.
# The data can then be loaded with the rails db:seed command (or created alongside the database with db:setup).
#
# Examples:
#
#   movies = Movie.create([{ name: 'Star Wars' }, { name: 'Lord of the Rings' }])
#   Character.create(name: 'Luke', movie: movies.first)
require 'date'

User.delete_all
Account.delete_all
Transaction.delete_all
Goal.delete_all
Bill.delete_all


u = User.create(fname: 'J', lname: 'P', password_digest: '#', session_token: '#', email: 'j@g.com', password: 'jacobp')

a = Account.create(account_type: "Checking", balance: 0, debit: true, inst: "Chase", label: "Personal", user_id: u.id)

t1 = Transaction.create(amount: 12.5, category: "Food", description: "Denny's", date: DateTime.new(2020, 2, 3, 4), notes: "pancakes", account_id: a.id)
t2 = Transaction.create(amount: 12.5, category: "Gas", description: "Shell", date: DateTime.new(2020, 4, 3, 4), notes: "fuel", account_id: a.id)

g = Goal.create(completed: false, goal_amt: 100, goal_category: "savings", notes: "i want a boat", title: "boat", account_id: a.id)

b = Bill.create(amount_due: 5, details: "Jen", due_date: DateTime.new(2020, 8, 8), paid: false, recurring: 0, user_id: u.id)