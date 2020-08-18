import React, {useState, useEffect} from 'react'
import AccountLineItem from './account_line_item'


export default function account_category({ accounts, category, logo }) {

  const [toggle, setToggle] = useState(false)

  const categorySubTotal = accounts.map((account) => (
      account.balance
    )).reduce((acc = 0, balance) => {
      acc + balance
    }, 0);

  const handleClick = () => {
    setToggle(() => (
      !toggle
    ))
  }


  return (
    
      <div className={`account-category ${toggle ? "active" : ""}`} onClick={handleClick} >
        <img src={logo} alt="image" className="category-icons" />
        <label className="account-category-label">{category}</label>
        <div className="category-subtotal">{categorySubTotal}</div>
      <div className="category-line-items">
        <ul>
          {accounts.map((account) => (
            <AccountLineItem account={account} key={account.id}/>
          ))}
        </ul>
      </div>
    </div>
  )
}
