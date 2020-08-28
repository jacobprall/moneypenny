# moneypenny

IMAGE

## Summary

Moneypenny is a single page web application inspired by Mint built using Ruby on Rails utilizing React.js and Redux architecture. 
Moneypenny allows users to:

- Create an account
- Log in and out
- Create finanical accounts to track their assets and liabilities.
- Update and delete their financial accounts
- Browse a daily briefing
- Add, edit and delete transactions
- View of graphical representation of their monthly financial transactions by category
- Add, edit and delete goals
- Add, edit and delete bills
- Create recurring bills

## Overall Structure

### Backend
The app was built using Ruby on Rails as the server and RESTful API. All data requests use AJAX and are fuliflled with JSON. The database utilizes postgreSQL, and associates are used to prefetch data in order to minimize SQL queries to teh database.

### Frontend
The frontend is built entirely in React and Javascript, while utilizing Redux's state architecture. React was chosen due to the speed and effiency of its virtual DOM. 

### Libraries
- React.js
- Redux
- Chart.js
- react-chartjs-2
- BCrypt for authorization
- figaro to store keys
- NYT API

## Primary Components
### User Authentication
User authentication is handled in Rails using BCrypt for password hashing. Passwords are never stored to the database. When users log in, the password provided is rehashed (with a salt) to be checked against the original encrypted password hash.


### Financial Accounts and Overview Page
Financial institutions don't have public APIs that allow third-party access. Therefore, users can create representations of their accounts in Moneypenny. These accounts can be edited easily, and give a net worth calculated at the bottom of the accounts component.

IMAGE
CODE

On the overview page, there is also a daily briefing utilizing the NYT public API. Below the accounts component is a visual representation of the month's transactions, broken down by category.
IMAGE

### Transactions
Users can add, edit and delete transactions in the transactions section. 
IMAGE


### Goals 
Users can create goals and assign them to specific accounts. Goals can be modified and deleted with ease.
IMAGE


### Bills
Users can also create and manage their bills. When set to recurring, paying a bill automatically creates a new bill for the next month. 
IMAGE

