import React, { useEffect, useState } from 'react'
import AccountCategory from './account_category'
import NetWorth from './net_worth'


export default function accounts_index({accounts, getAccounts}) {

  useEffect(() => {
    getAccounts()
  }, [])
  const categoryList = ['Cash', 'Credit Cards', 'Loans', 'Investments', 'Property']
  
  const accountCategories = (categoryList) => {
    const categories = {};
    categoryList.forEach((category) => {
      const categoryAccounts = accounts.filter((account) => (
        account.account_category === `${category}`
      ))
      categories[category] = categoryAccounts
    })
    return categories
  }

  const categories = accountCategories(categoryList)

  const categorySubTotal = (categoriesObj) => {
    const categorySubs = {};
    for (const category in categoriesObj) {
      categorySubs[category] = categoriesObj[category].map((account) => (
        Math.round(account.balance)
      )).reduce((acc = 0, balance) => (
        acc + balance
      ), 0)
    }
    return categorySubs
  }

  const categorySubs = categorySubTotal(categories)
  


  return (
    <div className='accounts-index-container'>
      <AccountCategory accounts={categories['Cash']} category="Cash" logo={window.money} catSub={categorySubs['Cash']}/>
      <AccountCategory accounts={categories['Credit Cards']} category="Credit Cards" logo={window.card} catSub={categorySubs['Credit Cards']}/>
      <AccountCategory accounts={categories['Loans']} category="Loans" logo={window.cap} catSub={categorySubs['Loans']}/>
      <AccountCategory accounts={categories['Investments']} category="Investments" logo={window.chart} catSub={categorySubs['Investments']}/>
      <AccountCategory accounts={categories['Property']} category="Property" logo={window.house}  catSub={categorySubs['Property']}/>
      <NetWorth accounts={accounts} />
    </div>
  )
}
