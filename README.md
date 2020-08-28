# moneypenny

![alt text](https://github.com/jacobprall/moneypenny/blob/master/app/assets/images/splash.png)

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

## Design Decisions
### Functional vs. OOP
In general, object-oriented program makes more sense to me, intuitively, than functional programming. Because of this, I challenged myself to create this app using only functional components. In the process, I learned a great deal about React hooks, and ultimately wrote less lines of code in a more efficient manner to emulate the functionality of a class component.


### Redux Hooks vs Redux Containers
Mid-way through this project, I realized the utility of skipping Redux container components in favor of using the useSelector and useDispatch hooks. This allowed clearer, concise code and allowed me to pass components into state.
![alt](https://github.com/jacobprall/moneypenny/blob/master/app/assets/images/noreduxcontainer.png)


### Modal and Redux State
Instead of using a modal component as a switch board that receives strings and loads a particular component on import, I decided to pass the necessary modal forms into state, allowing the modal component to directly display the component stored in state. This required sending a formType to state as well, to distinguish between a new form and an edit form. It also required creating a space for a "passed" account, transaction, goal or bill to be stored for the modal component to access.

![alt](https://github.com/jacobprall/moneypenny/blob/master/app/assets/images/modalcode.png)


## Primary Components
### User Authentication
User authentication is handled in Rails using BCrypt for password hashing. Passwords are never stored to the database. When users log in, the password provided is rehashed (with a salt) to be checked against the original encrypted password hash.


### Financial Accounts and Overview Page
Financial institutions don't have public APIs that allow third-party access. Therefore, users can create representations of their accounts in Moneypenny. These accounts can be edited easily, and give a net worth calculated at the bottom of the accounts component.
On the overview page, there is also a daily briefing utilizing the NYT public API. Below the accounts component is a visual representation of the month's transactions, broken down by category.
![alt](https://github.com/jacobprall/moneypenny/blob/master/app/assets/images/overview1.png)
![alt](https://github.com/jacobprall/moneypenny/blob/master/app/assets/images/overview2.png)
How net worth is calculated
![alt](https://github.com/jacobprall/moneypenny/blob/master/app/assets/images/networth.png)



### Transactions
Users can add, edit and delete transactions in the transactions section. 
![alt](https://github.com/jacobprall/moneypenny/blob/master/app/assets/images/transactions.png)


### Goals 
Users can create goals and assign them to specific accounts. Goals can be modified and deleted with ease.
![alt](https://github.com/jacobprall/moneypenny/blob/master/app/assets/images/goals.png)



### Bills
Users can also create and manage their bills. When set to recurring, paying a bill automatically creates a new bill for the next month. 
![alt](https://github.com/jacobprall/moneypenny/blob/master/app/assets/images/bills.png)


