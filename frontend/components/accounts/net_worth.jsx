import React, {useState} from 'react'
import commaFormat from '../../util/number_formatter'

export default function net_worth({accounts}) {
    let assets = accounts.filter((account) => (
      account.debit
    )).map((account) => (
      account.balance
    )).reduce((acc = 0, account) => (
      account + acc
    ), 0);

    let liabilities = accounts.filter((account) => (
      !account.debit
    )).map((account) => (
      account.balance
    )).reduce((acc = 0, account) => (
      account + acc
    ), 0);
    assets = assets.toFixed(2)
    liabilities = liabilities.toFixed(2)
    
    const netWorth = (assets - liabilities).toFixed(2)
  
  return (
    <>
    <ul className="net-worth">
      <li className="net-worth-li">
        <span className="net-worth-label">Assets</span>
        <span className="net-worth-assets">{`$${commaFormat(assets.toString())}`}</span>
      </li>
        <br/>
      <li className="net-worth-li">
        <span className="net-worth-label">Debts</span>
          <span className="net-worth-liabilities">{`$${commaFormat(liabilities.toString())}`}</span>
      </li>
        <br/>
        <li className="net-worth-li">
        <span className="net-worth-label">Net Worth</span>
          <span className={`net-worth-data ${netWorth > 0 ? 'green' : 'red'}`}>{`$${commaFormat(netWorth.toString())}`}</span>
      </li>
      </ul>
      </>

   
  )
}
