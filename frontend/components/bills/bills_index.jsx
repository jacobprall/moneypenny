import React, { useState } from "react";
import BillsLineItem from "./bills_line_item";
import BillsFormContainer from "./bills_form_container";
import { useSelector, useDispatch } from "react-redux";
import { openModal } from "../../actions/modal_actions";
import { requestBills } from '../../actions/bill_actions'
import commaFormat from '../../util/number_formatter'
import Calendar from 'rc-calendar'
import enUS from 'rc-calendar/lib/locale/en_US'
import { formatDate } from '../../util/date_util'


export default function bills_index() {
  const dispatch = useDispatch();
  const accounts = useSelector((state) =>
    Object.values(state.entities.accounts)
  );
  // console.log(accounts)
  
  const cash = accounts.filter((account) => {
    return account.account_category === "Cash";
  });


  const cashAmounts = cash.map((account) => account.balance)
  const sum = cashAmounts.reduce((acc, ele) => (acc + ele), 0);
  // console.log(sum)
  const sumFormatted = commaFormat(sum.toFixed(2).toString())
  

  const modalOpener = (formType, component, payload) => dispatch(openModal(formType, component, payload));
  const bills = useSelector((state) => Object.values(state.entities.bills));
  const userId = useSelector((state) => state.session.id)



  const renderBills = () => (
    bills.map((bill, i) => (
      <BillsLineItem bill={bill} key={i} />
    ))
  );
  
  const defaultValue =  new Date();


  const newBill = {
    'name': '',
    'amount': 0,
    'due_date': new Date(),
    'recurring': true,
    'user_id': userId
  };

  const calendar = (<Calendar 
    locale={enUS}
    style={{ zIndex: 1001 }}
    timePicker={null}
    disabledTime={null}
    dateInputPlaceHolder={formatDate(defaultValue)}
    showToday={false}
    />);


  return (
    <div className="bills-index-container">
      <div className="bill-page">
        <div className="bills">
          <div className="bills-header">
            <div className="bills-due-total">
              <p className="text-header-bills">Bills</p>
              <div className="header-center">
                <p className="cash-available">Cash Available</p>
                <p className="text-header-amount">${sumFormatted}</p>
              </div>
            </div>
          </div>
          <div className="add-bill">
            <button
              className="add-bill"
              onClick={() => modalOpener("new", BillsFormContainer, newBill)}>
                + Add Bill
            </button>
          </div>
          <div className="bills-list-container">
            <ul className="bills-list">{renderBills()}</ul>
          </div>
        </div>
        <div className="calendar">{calendar}</div>
      </div>
    </div>
  );
}
