# == Schema Information
#
# Table name: accounts
#
#  id           :bigint           not null, primary key
#  account_type :string           not null
#  balance      :decimal(8, 2)    not null
#  debit        :boolean          not null
#  inst         :string
#  label        :string           not null
#  created_at   :datetime         not null
#  updated_at   :datetime         not null
#  user_id      :string           not null
#
# Indexes
#
#  index_accounts_on_user_id  (user_id)
#
class Account < ApplicationRecord

  validates :account_type, presence: true, inclusion: { in: %w(Checking Savings Loan Credit_Card Cash) }
  validates :balance, :debit, :inst, :label, presence: true
  
  belongs_to :user,
  foreign_key: :user_id,
  class_name: :User 
  
  has_many :transactions,
  foreign_key: :account_id,
  class_name: :Transaction 

  has_many :goals,
  foreign_key: :account_id,
  class_name: :Goal


end
