import React from 'react'
import AccountLineItem from './account_line_item'


export default function account_category({ accounts, category}) {
  const categorySubTotal = account.map((account) => (
    account.balance
    )).reduce((acc = 0, balance) => {
      acc + balance
    });
  return (
    <div className="account-category">
      <label>{category}</label>
      <ul>
        {accounts.map((account) => {
          return <AccountLineItem account={account} />
        })}
      </ul>
      <div>{categorySubTotal}</div>
    </div>
  )
}
