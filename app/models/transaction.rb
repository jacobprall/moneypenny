# == Schema Information
#
# Table name: transactions
#
#  id          :bigint           not null, primary key
#  amount      :decimal(8, 2)    not null
#  category    :string
#  date        :datetime         not null
#  description :string
#  notes       :text
#  tags        :string
#  created_at  :datetime         not null
#  updated_at  :datetime         not null
#  account_id  :integer          not null
#
# Indexes
#
#  index_transactions_on_account_id  (account_id)
#
class Transaction < ApplicationRecord
CATEGORY_TYPES = [
  "Food & Dining",
  "Uncategorized",
  "Transportation",
  "Bills & Utilities",
  "Education",
  "Entertainment",
  "Fees & Charges",
  "Work Expense",
  "Home",
  "Income",
  "Miscellaneous",
  "Shopping",
  "Taxes",
  "Travel",
  "Personal Care",
  "Personal Supplies",
  "Health"
]
  attr_accessor :total

  default_scope { order('date DESC') }
  validates :amount, :description, :account_id, :category, :date, presence: true
  validates :category, inclusion: { in: CATEGORY_TYPES }
  belongs_to :account,
  foreign_key: :account_id,
  class_name: :Account

  has_one :user,
  through: :account,
  source: :user 


  
  
  include PgSearch
  multisearchable :against => [:description, :category]

end
