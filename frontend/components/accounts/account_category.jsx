import React, {useState, useEffect} from 'react'
import AccountLineItem from './account_line_item'
import { useSelector, shallowEqual} from 'react-redux'

export default function account_category({ accounts, category, logo, catSub, commaFormat }) {
  const [toggle, setToggle] = useState(false)
  

  // console.log(accountsChecker)

  const handleClick = () => {
    setToggle(() => (
      !toggle
    ))
  }


  return (
    
    <div className={`account-category ${toggle ? "active" : ""}`} onClick={handleClick} >
        <div className={`account-category-li ${toggle ? "active" : ""}`}>
          <img src={logo} alt="image" className="category-icons" />
          <span className="account-category-label">{category}</span>
          <span className="category-subtotal">{`$${commaFormat(catSub.toString())}`}</span>
        </div>
        
        <div className="category-line-items">
          <ul>
            {accounts.map((account, i) => (
              <AccountLineItem account={account} commaFormat={commaFormat} key={i}/>
            ))}
          </ul>
        </div>
   </div>
  )
}
