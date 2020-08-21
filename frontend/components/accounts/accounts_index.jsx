import React, { useEffect, useState } from 'react'
import AccountCategory from './account_category'
import NetWorth from './net_worth'


export default function accounts_index({accounts, commaFormat}) {

  // useEffect(() => {
  //   getAccounts()
  // }, [])

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
        account.balance
      )).reduce((acc = 0, balance) => (
        acc + balance
      ), 0).toFixed(2)
    }
    return categorySubs
  }

  const categorySubs = categorySubTotal(categories)
  


  return (
    <div className='accounts-index-container'>
      <AccountCategory accounts={categories['Cash']} category="Cash" logo={window.money} catSub={categorySubs['Cash']} commaFormat={commaFormat}/>
      <AccountCategory accounts={categories['Credit Cards']} category="Credit Cards" logo={window.card} catSub={categorySubs['Credit Cards']} commaFormat={commaFormat}/>
      <AccountCategory accounts={categories['Loans']} category="Loans" logo={window.cap} catSub={categorySubs['Loans']} commaFormat={commaFormat}/>
      <AccountCategory accounts={categories['Investments']} category="Investments" logo={window.chart} catSub={categorySubs['Investments']} commaFormat={commaFormat}/>
      <AccountCategory accounts={categories['Property']} category="Property" logo={window.house} catSub={categorySubs['Property']} commaFormat={commaFormat}/>
      <NetWorth accounts={accounts} />
    </div>
  )
}
